---
id: SPEC-agent-prompt
revision: 2
status: approved
---

# Navigate from a hydrated Agent Card to its Prompt Card

## Objective and user value

Declare an Agent's Prompt once, then obtain its hydrated Prompt Card through
that Agent. A Service need not list the Prompt separately. An explicitly
aliased Prompt remains independently accessible through state.

```python
card = state["agent"]
assert isinstance(card, AgentCard)
agent: AgentCard = card
prompt: PromptCard = agent.prompt()
ref: CardRef = prompt.ref

# When explicitly aliased in the Service:
card = state["my_prompt"]
assert isinstance(card, PromptCard)
prompt = card
```

Aliases are runtime data. Generic lookup returns a union of hydrated Card
types; narrowing makes the exact Card type available to the type checker.
Existing typed accessors return their exact Card type directly.

This replaces revision 1's proposed state-level `agent_prompt` / `agentPrompt`
methods. WyrdState owns alias lookup and its hydrated graph; the hydrated Agent
exposes its resolved Prompt relationship.

## Approval and revision history

- Revision 1, approved 2026-10-14: transitive Prompt access without a redundant
  Service component, originally through state-level methods.
- Revision 2, approved by owner direction in this conversation on 2026-10-15:
  `AgentCard.prompt()` returns `PromptCard`, with its identity at `.ref`;
  explicit Prompt aliases remain usable; inline and file-based Prompts become
  hydrated Prompt Cards. The owner's final "agree" approves deriving the
  inline Prompt name from its Agent, in the same space, with independent
  registry-managed versions and no implicit sharing across different Agents.
- Typing clarification requested by the owner: Python and TypeScript Card
  lookup must have precise public types, sound narrowing and compile-time
  regression proof, rather than relying on annotations over untyped values.

## Current behavior and authority changes

- `wyrd-loader::resolve_tree` already loads file-path targets as full Card
  envelopes and projects them to sibling submissions. Those Prompts retain
  their authored metadata and can already be registered/hydrated transitively.
- Server `collect_inline_ref` accepts inline content without separate identity.
  Protocol doctrine 17 and Reference forms explicitly leave inline children
  embedded and unregistered. Inline Prompts currently have no CardRef.
- Shared WyrdState has typed Agent/Prompt indexes; its internal-gated
  `agent_prompt` wrapper and unconditional `resolve_agent_prompt` return a
  bare Prompt spec, not a hydrated Prompt Card relationship.
- Python already exposes `prompt_ref` and a resolved `prompt` property, but
  that property returns a runtime Prompt. TypeScript exposes a declarative
  Agent envelope. Generic hydrated lookup/navigation is missing.
- This revision deliberately replaces the embedded-only rule for inline
  Prompts of **registered Agent Cards**. Update the protocol and public
  rationale in the implementation change. Other inline kinds and standalone
  local Workflow loading retain their existing semantics.

## Required behavior

### REQ-001 Agent-owned Prompt access

- A hydrated Agent Card exposes synchronous `prompt()` returning its hydrated
  Prompt Card with its exact registered identity and payload. Python supports
  the example above; Rust/TypeScript expose equivalent typed navigation.
- Prompt Cards expose `.ref` in Python/TypeScript and an idiomatic Rust
  reference accessor, carrying the actual resolved version and UID.
- An ergonomic accessor for the Agent's reference is `prompt_ref`
  (`promptRef` in TypeScript). The durable wire field remains `AgentSpec.prompt`.
- Do not add public WyrdState `agent_prompt` / `agentPrompt` methods. Internal
  consumers, including `Run::invoke`, use the same resolved Card relationship.

### REQ-002 Register and hydrate every Agent Prompt

- Ref, file-path, and inline authoring all produce a registered Agent linked
  to a registered Prompt Card that is included in the hydrated state graph.
- Paths retain the existing full-Card-envelope loader contract: authored
  metadata, containing-file-relative resolution, sandbox/depth checks and
  same-request registration. The server never reads local file paths.
- Inline materialization happens at the server registration boundary, including
  direct HTTP input; it must not depend on a particular SDK synthesizing a Card.
- The Prompt participates in the existing composite Card transaction/lifecycle,
  registration outcomes and server-derived Agent relationship. Persist the
  Agent's exact UID-bearing Prompt reference; the existing hydrator follows it.
- State loading and `prompt()` never register. Cards remain independent;
  navigating a relationship does not imply ownership or cascading deletion.

### REQ-003 Stable inline identity and independent versioning

- For Agent `<agent-name>`, name its inline Prompt `<agent-name>-prompt` in
  the Agent's effective space and authenticated tenant. Invalid derived names
  fail normal identity validation; never silently truncate or hash the name.
- Use the Prompt's own normal registry-managed version line, not the Agent's
  version. Unchanged Prompt content reuses its exact identity; changed content
  follows existing registry versioning rules.
- Each Agent version pins its selected Prompt version. Later changes never
  change the Prompt returned through an already registered Agent version.
- Identical inline content in differently named Agents is not implicitly
  shared. Explicit Prompt refs/paths provide intentional reuse.
- File-backed and referenced Prompts retain authored identities/version intent.
  Existing registry concurrency/conflict rules also govern a generated name
  coinciding with an independently authored Prompt line. Add no reserved
  namespace, overwrite behavior or separate versioning policy.

### REQ-004 Generic alias lookup and explicit Prompt aliases

- Python `state[alias]` and `state.get(alias)` return the kind-appropriate
  hydrated Card. Rust/TypeScript expose equivalent generic typed alias lookup;
  existing typed accessors stay coherent with it.
- A Service need not declare the Agent's Prompt separately. Other components,
  such as a Model, are allowed.
- An explicit Prompt alias and `agent.prompt()` expose the same exact Card
  identity/payload. Python object identity is not a guarantee.
- Reuse existing alias indexing; no generated Service component or additional
  alias namespace is required for the transitive Prompt dependency.

### REQ-005 Local reads and stable failures

- Hydrated alias lookup, `agent.prompt()` and `prompt.ref` perform no filesystem
  or network IO, registration, or version selection.
- Strict generic lookup, including `get`, retains
  `WYRD_SDK_404_UNKNOWN_ALIAS`; typed mismatch retains
  `WYRD_SDK_400_CARD_KIND_MISMATCH`. `get` is strict, not nullable.
- Missing/wrong-kind targets, unresolved paths and an old inline-only bundle
  without registered Prompt identity fail with
  `WYRD_SDK_400_INVALID_STATE_BUNDLE`. Never fabricate identity or repair a
  bundle with hidden writes. Re-register and rehydrate old graphs that lack
  the identity; do not mutate existing Card versions.
- Preserve authorization, tenant isolation, atomic Card writes, stable
  validation/conflict errors, idempotency and non-blocking audit.

### REQ-006 Consumer closure

- Ship Rust, Python and TypeScript SDK behavior, public typing/exports,
  docs/examples and applicable generated contracts together.
- Python indexing/get expose a public union of hydrated Card types that narrows
  with `isinstance`; TypeScript generic lookup exposes a discriminated union
  that narrows by Card kind. An arbitrary runtime alias cannot be assumed to
  have a particular kind from its string or an assignment annotation.
- Existing typed state accessors return their exact hydrated Card types.
  After narrowing or typed retrieval, `AgentCard.prompt()` is typed as
  `PromptCard`, and `PromptCard.ref` as `CardRef`, without optional or
  bare-runtime-Prompt return types.
- All returned Card kinds, including directly aliased Prompts, retain their
  specific members and public importable types. Do not hide missing types with
  `Any`/`any`, a broad object/dictionary, casts, or an unchecked generic type
  parameter that lets callers claim an arbitrary Card kind.
- Compile-only contracts prove positive inference/narrowing and rejection of
  incompatible Card assignments or members in both languages. Runtime tests
  separately prove returned objects match these public declarations.
- Update changed Python `.prompt` property consumers to the new PromptCard
  method contract. Preserve access to the underlying Prompt's capabilities.
- The old accessor WIP commit is not an implementation base or approved API.
  External repositories are outside this change.

## Invariants and non-goals

- INV-001 Durable registration, version selection, relationships, policy and
  audit remain server-owned. SDKs project shared `wyrd-client` behavior.
- INV-002 Pure `wyrd-spec` Cards/specs stay declarative, IO-free and PyO3-free.
  Hydrated SDK Cards may retain local resolved dependencies, without hidden
  registry clients or writes.
- INV-003 Reuse the registration transaction, canonical reference-slot
  inventory, typed state index and graph hydrator. No second graph store,
  traversal, transport, loader or language-specific resolver.
- INV-004 No unrelated inline-kind redesign, standalone Workflow registration,
  cascading deletion, destructive migration, compatibility aliases, new Cargo
  features or unrelated Bifrost/UI changes.

## Acceptance obligations and evidence

- AC-001 Real-server HTTP and SDK/apply proofs register inline Agent content,
  return/persist its independent Prompt and exact relationship, and preserve
  path/ref registration.
- AC-002 Registration journeys cover replay, changed Prompt content, old Agent
  version pinning, different Agents with identical content, authored path
  metadata, invalid derived names/conflicts, permission denial and tenant
  separation under existing public registration semantics.
- AC-003 Rust/Python/TypeScript journeys register/hydrate a Service without a
  redundant Prompt component, navigate to its PromptCard, and inspect its
  authentic reference. Cover inline/path/ref forms and explicit Prompt alias
  access. Prove reads after server shutdown and preserve invocation behavior.
- AC-004 Negative proofs cover unknown aliases, typed mismatch, malformed or
  missing dependencies and old inline-only bundles without hidden IO/writes.
  Public negative flows get journeys; malformed local state may use unit
  tests because it has no cross-boundary state.
- AC-005 Relevant format/lint/typing/generated/boundary/owner checks pass.
  Python and TypeScript compile-only contracts prove precise typed accessors,
  generic lookup narrowing, Agent-to-PromptCard-to-CardRef types and negative
  type cases through the normal public imports and typecheck lanes.
  Complete affected journeys and end-to-end proof run once on the integrated
  candidate at change review.

## Open material decisions

None. Private holder composition, lifetimes, helpers and fixtures remain
implementation-owned.

## Authority links

- [Repository standards](../../../AGENTS.md)
- [Agent rules](../../../architecture/agent-rules.md)
- [Protocol authority](../../../architecture/wyrd-design.md)
- [Public doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Architecture patterns](../../../architecture/references/architecture/patterns.md)
- [Spec-driven workflow](../../../architecture/references/languages/spec-driven-development.md)
