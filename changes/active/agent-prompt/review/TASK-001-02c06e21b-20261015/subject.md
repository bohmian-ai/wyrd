# Immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt
Base (parent of first implementation commit c90f4c7d4): `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`
Candidate: `02c06e21bd5fd613341935a843fabe5350ea0a9d`
Review full `git diff c1aac2d132b19848ca1940c3907e1fa68a8d08a5 02c06e21bd5fd613341935a843fabe5350ea0a9d`; HEAD must stay at candidate.
Spec: `changes/active/agent-prompt/spec.md` revision 2 approved.
Original task and recorded verification: `changes/active/agent-prompt/tasks/TASK-001-agent-prompt.md`.
First review; no previous findings or remediation.

## Navigation starting points (expand independently)

- Server registration: `crates/wyrd/wyrd-server/src/components/cards/service.rs`: `register_card`, `RegistrationWriter::write`, `persist_node`, `inline_prompt_submission`; surrounding resolve, graph, version-lock/idempotency and HTTP route owners.
- Shared client: `crates/shared/wyrd-client/src/state.rs`: `TypedCards`, `HydratedAgent`, `HydratedCard`, `HydratedStateIndex`, `WyrdState::agent/get/prompt`; `observe/invoke.rs::Run::invoke`; existing Cards graph hydration and bundle validation.
- Card/Python holders: `crates/wyrd/wyrd-cards/src/agent.rs` and `prompt.rs`; SDK `src/state/mod.rs`, `hydrator.rs`, authored cards/prompt/state stubs and exports.
- TypeScript: native `src/cards.rs`, public `wyrd/src/index.ts` hydrated AgentCard/PromptCard/HydratedCard and WyrdState; generated native declarations.
- Tests: server `pg_card_registration_route` integration; Rust SDK `register_and_hydrate` and existing gateway invocation; Python register/hydrate and state runtime/typing; TS register/hydrate and compile-only Card contracts; shared fixtures and CLI lifecycle consumer.
- Authority: AGENTS.md, architecture/agent-rules.md, reference router; protocol and doctrine diffs; spec-driven development, implementation execution, testing and maintainer references. Select full applicable language/domain/security references.

Verification is claimed in the task's appended evidence; inspect actual proof rather than trusting PASS labels. TDD history limitations are recorded there. Do not check out a baseline or run older code to assign blame. Do not change reviewed source or execute builds/tests concurrently with other reviewers. Static review first; request orchestrator coordination for a necessary runtime check.

## Changed paths

```text
architecture/wyrd-design.md
architecture/wyrd-doctrine.mdx
changes/active/agent-prompt/tasks/TASK-001-agent-prompt.md
crates/shared/wyrd-client/src/observe/invoke.rs
crates/shared/wyrd-client/src/state.rs
crates/wyrd/wyrd-cards/src/agent.rs
crates/wyrd/wyrd-cards/src/prompt.rs
crates/wyrd/wyrd-cli/tests/card_lifecycle.rs
crates/wyrd/wyrd-server/src/components/cards/service.rs
crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs
docs/src/content/docs/understand/cards-versions-and-relationships.md
fixtures/cards/agent_prompt/desk-prompt.yaml
fixtures/cards/agent_prompt/inline-agent.yaml
fixtures/cards/agent_prompt/path-agent.yaml
fixtures/cards/agent_prompt/prompt-desk.yaml
fixtures/cards/agent_prompt/ref-agent.yaml
sdks/wyrd-sdk-python/python/wyrd/cards/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/prompt/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi
sdks/wyrd-sdk-python/src/state/hydrator.rs
sdks/wyrd-sdk-python/src/state/mod.rs
sdks/wyrd-sdk-python/tests/fixtures/bundles/complete/cards/inline_agent/card.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/complete/cards/inline_agent/relationships.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/inline-only-agent/cards/inline_agent/card.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/inline-only-agent/cards/inline_agent/relationships.yaml
sdks/wyrd-sdk-python/tests/integration/state/test_state_journey.py
sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py
sdks/wyrd-sdk-python/tests/typecheck/state_contracts.py
sdks/wyrd-sdk-python/tests/unit/runtime/agent/test_agent_journey.py
sdks/wyrd-sdk-python/tests/unit/state/test_hydration.py
sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs
sdks/wyrd-sdk-ts/native/src/cards.rs
sdks/wyrd-sdk-ts/wyrd/index.d.cts
sdks/wyrd-sdk-ts/wyrd/index.d.ts
sdks/wyrd-sdk-ts/wyrd/src/index.ts
sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts
sdks/wyrd-sdk-ts/wyrd/tests/types/card-types.test-d.ts
```
