---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-agent-prompt
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, INV-001, INV-002, INV-003, INV-004, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
---

# Register Agent Prompts and expose hydrated Prompt Card navigation

## Outcome and Value

An application declares an Agent's Prompt inline, by file path or by CardRef,
registers and hydrates the Service, then obtains the registered PromptCard
through `agent.prompt()` and its authentic identity through `prompt.ref`.
Generic state lookup returns hydrated Cards; explicitly aliased Prompts remain
independently accessible. This task owns the complete registration-to-navigation
outcome and its Rust, Python and TypeScript consumer/test closure.

Use `$wyrd-implement`. This replaces the r1 state-accessor task. Do not
cherry-pick `cca791ab6`: its API is superseded and its Python stub path is stale.

## Owners, Scope, Consumers, and Prohibited Changes

Server registration owns materialization, durable identity/version selection
and exact references; the existing loader retains file-path handling. Direct
HTTP and SDK input must agree. The shared client owns hydrated SDK behavior;
Python/TypeScript project it across their runtime boundaries. Pure envelopes
and specs remain declarative; hydrated Card holders/views may retain already
resolved local dependencies without acquiring hidden IO or registry clients.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Composite registration | `wyrd-server/src/components/cards/service.rs::register_card`, `RegistrationWriter::write`, `persist_node` | `EffectiveSpecs::resolve`; `tests/integration/pg_card_registration_route.rs` | Inline content produces no Prompt registration node/outcome | Extend existing server registration transaction/lifecycle | No separate saga, transaction or registration owner |
| Typed normalization | `components/cards/resolve.rs::collect_inline_ref`, `bind_card_references`; `wyrd_spec::refs::ReferenceSlotVisitor` | Registration writer and public HTTP integration target | Inline is accepted without identity or derived Prompt relationship | Extend canonical graph/reference normalization for registered Agent Prompts | No second slot inventory or lineage graph |
| Versioning and replay | `service.rs::persist_node`, `resolve_existing`; `wyrd_sql::queries::cards::resolve_version` | `register_card` replay path; Rust SDK registration/hydration integration target | No generated identity or independent version for inline child | Reuse registry deduplication, locks, version selection and idempotency | No alternate version store or SDK-generated UID |
| Path authoring | `wyrd-loader/src/resolve.rs::resolve_tree`, `PathResolver::resolve`; `wyrd-client::cards::Cards::register_from_path` | CLI `card.rs::apply`; Rust SDK register-and-hydrate journeys | Existing path behavior needs regression proof | Preserve file metadata, sandbox/depth checks and sibling intent | No new loader/parser or server filesystem access |
| Graph hydration | `wyrd-client::cards::hydrate::CardGraphHydrator`, `graph.rs::GraphTraversal` | Rust SDK `tests/integration/register_and_hydrate.rs` | Generated Prompt must be reachable through normal relationships | Publish ordinary Prompt outcomes/relationships for existing hydrator | No new traversal, graph store or Prompt-specific bundle format |
| Typed alias lookup | `wyrd-client/src/state.rs::WyrdState`, `HydratedStateIndex`, `card`, `typed_key`, typed accessors | Rust SDK register/hydrate integration target; Python `src/state/mod.rs::card`, `agent`; TS state wrapper | Generic retrieval exposes envelopes, not navigable hydrated Cards; Python lacks get/indexing | Extend current alias/typed index with generic hydrated lookup | A local relationship-bearing holder/view is justified because pure envelopes cannot retain resolved graph dependencies; no new graph store or traversal |
| Agent relationship | `state.rs::resolve_agent_prompt`, typed Agent/Prompt maps | `observe/invoke.rs::Run::invoke`; Python `PythonStateHydrator::hydrate_agents` | Shared resolver returns bare spec; Python attaches runtime Prompt | Expose registered PromptCard through hydrated Agent in shared Rust owner | No per-language resolver or public state-level agent_prompt |
| Prompt identity/payload | Existing typed PromptCard and `as_card_ref`; `wyrd-cards/src/prompt.rs::from_card` | Python prompt stubs; TS `RegisteredPromptCard` | No consistent hydrated PromptCard `.ref` surface | Expose authoritative held Card identity and preserve Prompt capabilities | No fabricated UID or alternate durable Card shape |
| Python projection | `src/state/mod.rs::agent`, `cached_holder`, `PythonStateHydrator::hydrate_agents`; existing `PyAgentCard::prompt`, `prompt_ref` | Registration/hydration journey, state hydration tests, authored cards/state/prompt stubs | `.prompt` property returns runtime Prompt, not callable PromptCard; no generic lookup | SDK-owned wrappers, indexing/get, typed stubs/exports and consumer updates | Existing owner-crate Python wrappers are migration state; new/materially relocated behavior belongs in Python SDK |
| TypeScript projection | `native/src/cards.rs::agent`; public `src/index.ts::WyrdState` | `tests/integration/register-and-hydrate.test.ts` | Plain envelope retains only wire reference | Thin typed hydrated SDK projection over shared native owner | Foreign-runtime projection earns a holder, not an independent JSON graph resolver |
| Public static typing | Authored Python Card/state stubs; TypeScript public wrapper/declarations | Python `tests/typecheck/` and `pyproject.toml::tool.ty.src`; TS `tests/types/card-types.test-d.ts` and `tsconfig.json`; `py:typecheck` / `ts:typecheck` task bodies | Generic hydrated lookup and navigation lack precise public return types and compile-only proof | Extend existing typing owners and compile-only contract suites for every returned Card kind, narrowing and exact relationship types | Reuse current ty/tsc lanes; no alias-specific code generation, new typecheck framework or unchecked generic assertion |
| Invocation | `wyrd-client/src/observe/invoke.rs::Run::invoke` | State Agent lookup and internal resolver consumers | New Card navigation and invocation could diverge | Consume same resolved PromptCard, preserving gateway/run correlation | No new invocation runtime |

Paths are relative to their named owners and are not a private implementation
allowlist. Trace changed `.prompt` consumers before editing. Keep the durable
`AgentSpec.prompt` field name. Runtime Prompt, pure specs and hydrated SDK Cards
remain distinct roles. No unrelated inline-kind redesign, local Workflow
registration, heavy-artifact changes, permission/audit changes, destructive
migration, new features or external repository edits.

## Approach

1. Update protocol/doctrine rules explicitly superseded by r2, scoped to inline
   Prompts of registered Agent Cards.
2. Extend existing server registration to materialize inline Prompts and select
   their independently versioned exact identities, preserving transaction,
   replay, concurrency, graph root and outcome behavior.
3. Extend existing shared typed state projection for generic hydrated Card
   lookup and Agent-to-PromptCard navigation over the registered graph.
4. Expose identity/payload without IO and route invocation/internal consumers
   through that same resolved relationship.
5. Project through Python/TypeScript, preserve Rust's thin SDK boundary, and
   keep generic lookup, explicit aliases and existing typed accessors coherent.
6. Update affected callers, docs/examples, authored Python stubs/exports and
   source-generated native declarations; add sound lookup/narrowing contracts
   to the existing Python/TypeScript compile-only suites.
7. Prove registration, replay/version pinning, SDK navigation, offline access,
   negative flows and invocation regression using the focused commands below.

## Ordered Implementation Scenarios

The named tests below are planned additions to inspected existing modules and
confirmed integration targets. Each command is recorded in Verification.

### Scenario 1 — Inline registration produces an independent Prompt

**Behavior.** Authenticated HTTP inline Agent input returns both Cards, one
root, a Prompt outcome, and an exact persisted Agent-to-Prompt ref/relationship.
REQ-002, INV-001/003, AC-001.

**RED.** Add planned
`pg_card_registration_route::inline_agent_prompt_materializes`; run A. The
Prompt outcome/ref assertion fails because current registration leaves it inline.

**GREEN.** Extend the existing server registration owner to materialize the
child in the Card transaction and normal active/blob lifecycle.

**REFACTOR.** Reuse canonical normalization/graph owners; keep A green without
introducing a separate registration saga.

### Scenario 2 — Names, replay and versions stay stable

**Behavior.** Unchanged registration reuses the exact Prompt. Changed content
uses its normal new version; old Agent versions remain pinned. Identical
content in different Agents produces distinct Prompt lines. REQ-003, AC-002.

**RED.** Add planned Rust SDK journey
`register_and_hydrate::agent_prompt_registration_identity`; run B. Extend its
replay/version/pinning assertions incrementally and observe each missing
behavior before implementation. Use normal valid version intents; retain
exact-pin conflict behavior.

**GREEN.** Complete the existing independent versioning/deduplication path,
including concurrent registration under normal registry rules; rerun A and B.

**REFACTOR.** Remove duplicate version/identity calculations while preserving
existing registry ownership and immutable exact refs.

### Scenario 3 — Path/ref and refusal behavior remain coherent

**Behavior.** Paths preserve authored Prompt metadata and explicit refs stay
exact. Invalid derived names, conflicting exact identities, under-privileged
writes and cross-tenant refs fail under existing catalog semantics without
partial Card writes. REQ-002/003, INV-001/004, AC-001/002.

**RED.** Extend A's HTTP proof and B's SDK journey for these edges/refusals.
Establish RED for new materialization branches. Already-correct path/ref and
security behavior supplies regression proof, not manufactured failures.

**GREEN.** Preserve loader validation, RLS, permissions, locks, transaction
rollback and non-blocking audit while applying them to generated children.
Rerun A and B through all cases.

**REFACTOR.** Reuse existing validation/transaction helpers; no manual tenant
filters, alternate audit writers or per-kind transports.

### Scenario 4 — The Agent returns its exact hydrated Prompt Card

**Behavior.** Inline/path/ref registration and hydration yield an Agent whose
`prompt()` returns PromptCard with the registry's exact identity/payload.
Invocation uses the same relationship. REQ-001, INV-001/002/003, AC-003.

**RED.** Add planned Rust SDK journey
`register_and_hydrate::hydrated_agent_prompt_card_navigation` using R. First
prove the missing public API/type surface, then its runtime identity/payload.
As each runtime projection is added, use planned Python
`test_hydrated_agent_prompt_card_navigation` via P and TypeScript
`hydrated agent prompt card navigation` via T; observe the missing API/behavior
rather than a setup error.

**GREEN.** Extend the shared owner and thin projections to return registered
Cards, preserving access to the underlying Prompt capabilities. Route invocation
through that same relationship and rerun implemented R/P/T proofs.

**REFACTOR.** Remove duplicated relationship resolution; keep pure envelopes
free of retained service clients and avoid parallel graphs/caches.

### Scenario 5 — Aliases and Agent navigation agree offline

**Behavior.** Agent navigation works without a redundant Prompt component.
With an explicit `my_prompt` alias, both paths yield the same exact identity
and payload. Python indexing/get, Rust/TS generic lookup and typed accessors
agree. Reads work after server shutdown. REQ-004/005, AC-003.

**RED.** Extend R/P/T with explicit-alias comparisons, lookup forms and reads
after server shutdown. Observe the missing generic lookup/navigation behavior.

**GREEN.** Reuse the existing alias index and hydrated relationship; add no
implicit Service component, network fetch, registration or version selection.
Rerun R/P/T including scenario 4.

**REFACTOR.** Remove duplicate conversion/resolution while retaining valid
runtime-owned Card lifetimes. Do not require Python object identity where exact
Card identity is sufficient.

### Scenario 6 — Invalid lookup and graph state fail without hidden repair

**Behavior.** Unknown strict lookup and typed mismatch retain existing error
codes. Missing/wrong-kind dependencies, unresolved paths and old inline-only
state fail as invalid bundle, never fabricated identity or hidden writes.
REQ-005, AC-004.

**RED.** Extend R/P/T for public alias/kind refusals. Add focused owner-local
state tests for malformed dependencies, recording their exact commands when
selected. Malformed local state has no cross-boundary state and is cleaner to
force in-process. Observe missing validation rather than generic exceptions.

**GREEN.** Enforce the shared graph/identity invariant and project stable
errors. Update genuine old inline-only fixtures to the registered-Card contract
without weakening their original negative coverage. Rerun all prior scenarios.

**REFACTOR.** Reuse validation/error catalog and existing state fixture owners;
no compatibility resolver, duplicate graph walker or weakened tests.

## Acceptance Criteria

- A proves direct HTTP parity, outcomes, exact persisted references,
  relationships, lifecycle and registration refusal/rollback behavior.
- B proves stable naming/replay/version pinning, no implicit sharing across
  different Agents, file metadata/ref regression, and normal hydration of the
  generated Prompt without an additional Service component.
- Protocol/doctrine no longer claim these Agent inline Prompts stay embedded;
  unrelated inline and local Workflow contracts remain intact.
- No public state-level Prompt accessor or external repository edit is added.
- R/P/T prove all authoring forms, authentic PromptCard refs/payloads, explicit
  alias agreement and local reads after shutdown.
- Python indexing/get return an explicit hydrated Card union that narrows with
  `isinstance`; TypeScript generic lookup returns a discriminated union that
  narrows by literal Card kind. Do not infer a kind from an arbitrary alias.
- Typed state accessors return exact Card types. Following typed retrieval or
  narrowing, `agent.prompt()` is precisely PromptCard and `.ref` is CardRef.
  Explicit Prompt aliases and other returned kinds preserve their members.
- Compile-only positive and negative contracts prove these public lookup and
  navigation types. Reject incompatible assignments/members without hiding
  errors behind Any/any, broad objects, casts or unchecked generic parameters.
  R/P/T separately prove runtime objects match those declarations.
- `prompt_ref`/`promptRef` exposes reference identity; `prompt()` exposes Card.
  The durable field name `AgentSpec.prompt` remains unchanged.
- Invocation retains config/tools handling, gateway routing/errors and Card/Run
  correlation through the shared relationship; regression command I proves
  normal execution, permission refusal and non-Agent refusal.
- R/P/T and focused malformed-state tests cover AC-004 with stable errors.
- Affected callers, public exports/stubs/declarations and examples are updated;
  new/materially relocated PyO3 belongs in the Python SDK boundary.

## Expected Write Set and Consumer Closure

- Server `src/components/cards/` registration owners; `wyrd-spec` graph/ref
  contracts and `wyrd-sql` registration support only where required.
- Existing client registration response consumers if needed; loader path
  behavior is regression scope, not replacement machinery.
- Server registration integration tests, Rust SDK registration/hydration
  journey owner and minimal necessary shared fixtures.
- `architecture/wyrd-design.md` doctrine 17, Agent, registry/authoring and
  identity guidance as affected; matching public doctrine and source-generated
  artifacts where contracts change. No unrelated architecture rewrite.
- `crates/shared/wyrd-client/src/state.rs`, hydrated holder/projection owners
  as required, `src/observe/invoke.rs`, and Rust SDK exports/tests.
- Python SDK state/Card projections, public exports and current authored
  `python/wyrd/state/__init__.pyi`, `python/wyrd/cards/__init__.pyi`,
  `python/wyrd/prompt/__init__.pyi`; affected runtime/state tests and examples.
  The deleted `python/wyrd/stubs/state.pyi` is not a target.
- TS `native/src/cards.rs`, public `wyrd/src/index.ts`, source exports,
  generated native declarations and owning runtime tests.
- Existing Python `tests/typecheck/` and TS `wyrd/tests/types/` compile-only
  contract suites, already included by their package typecheck configurations.
  Extend the existing test homes; fixture structure stays implementation-owned.
- Existing state/invocation consumers and repository docs/examples. Generated
  wire artifacts change only through their source generator where affected.
- External `wyrd-example-ts` changes are caller-owned, outside this repository.

## Verification and Evidence

A — planned HTTP test in confirmed `wyrd-server` integration target:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_card_registration_route::inline_agent_prompt_materializes)"'
```

B — planned journey in confirmed `wyrd-ai` integration target:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=register_and_hydrate::agent_prompt_registration_identity)"'
```


R — planned journey in confirmed `wyrd-ai` integration target:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=register_and_hydrate::hydrated_agent_prompt_card_navigation)"'
```

P — planned test in existing Python integration module; build SDK first:

```bash
mise run py:setup
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_register_and_hydrate.py::test_hydrated_agent_prompt_card_navigation'
```

T — planned test in existing Vitest integration file; build both addons first:

```bash
mise run ts:build
mise run ts:build:testing
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/register-and-hydrate.test.ts -t "^hydrated agent prompt card navigation$"'
```

I — existing invocation regression, confirmed in the Rust SDK journey target:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=gateway_inference::run_invokes_its_agent_through_the_gateway)"'
```

Run I after changing invocation consumption and retain it as regression proof;
it does not need a manufactured RED when execution behavior is unchanged.

### Static typing proof

Typing declarations and compile-only cases use static proof, not a manufactured
runtime RED. Extend the existing Python `tests/typecheck/` contracts and TS
`tests/types/` contracts; both are included by inspected package configuration.

Prove exact typed accessor returns, generic union return types, Python
`isinstance` narrowing, TS Card-kind narrowing, `AgentCard.prompt()` returning
PromptCard and `.ref` returning CardRef. Include directly aliased Prompts and
other lookup kinds. Check that incompatible assignments and kind-specific
members are rejected. Use existing checker-supported expected-diagnostic
assertions for negative cases, with unused assertions rejected; never suppress
real production errors or use casts to make a positive example pass.

Exact static-proof lanes:

```bash
mise run py:typecheck
mise run ts:typecheck
```

These commands check public imports and authored/generated declarations as
consumers see them. Keep runtime P/T assertions as separate native parity proof.

Record selected tests and scenario RED/GREEN results. Additional named
state/invocation regression tests require exact focused commands in execution
evidence. Read tracing before diagnosing failures. Run Cargo-backed work
sequentially with the existing minimal feature policy.

After focused proof: `mise run fmt`, `mise run lints`,
`mise run check:deps`, `mise run codegen:check`,
`mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`,
`mise run ts:napi:check`. Add `mise run check:tenant-isolation` if SQL/query
paths change. If HTTP schemas change, prove served OpenAPI through
`mise run test:principals:integration` per AGENTS.md. Generate native
declarations through `ts:build`; author Python stubs directly. Architecture,
docs and declarations use static consistency, typing and drift proof rather
than artificial RED. Run `mise run docs:check` if docs-site files change and
the owning example task if examples change. Complete affected journey suites
run once on the integrated candidate at change review.

## Material Stop Conditions

Stop for specification authority if correctness requires different naming or
versioning, altered existing conflict/security/audit/transaction guarantees,
registration on lookup/load, fabricated UIDs, mutation of old Card versions,
active-record specs, unrelated inline/Workflow changes or destructive migration.
Replacement graph/lifecycle/transport owners, new dependencies/features, or
public API changes beyond the approved navigation/lookup contract require
material justification rather than private invention.

## Authority Links

- [Approved spec r2](../spec.md)
- [AGENTS.md](../../../../AGENTS.md), especially §§5, 7–12, 14, 16
- [Agent rules](../../../../architecture/agent-rules.md)
- [Protocol](../../../../architecture/wyrd-design.md)
- [Doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Architecture patterns](../../../../architecture/references/architecture/patterns.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A: HTTP parity, outcomes, exact refs, relationship, lifecycle, refusal/rollback | `wyrd-server/src/components/cards/service.rs` `RegistrationWriter::write` + `inline_prompt_submission` (reuses `persist_node` in the Card transaction) | A `pg_card_registration_route::inline_agent_prompt_materializes` | PASS |
| B: naming, replay, version pinning, no cross-Agent sharing, authored-line clash, tenant separation, path/ref regression, hydration without Prompt component | Same server owner; fixtures `fixtures/cards/agent_prompt/` | B `register_and_hydrate::agent_prompt_registration_identity` | PASS |
| Doctrine/protocol scoped to registered Agent inline Prompts | `architecture/wyrd-design.md` doctrine 17, Prompt/Agent, pre-registration, Reference forms; `architecture/wyrd-doctrine.mdx`; `docs/.../cards-versions-and-relationships.md` | `mise run docs:check` | PASS |
| No public state-level Prompt accessor; no external repo edit | `wyrd-client/src/state.rs` (`agent_prompt`/`resolve_agent_prompt` removed) | diff audit | PASS |
| R/P/T: all authoring forms, authentic refs/payloads, explicit alias agreement, reads after shutdown | Rust `HydratedAgent`/`HydratedCard`, `WyrdState::agent`/`get`; Python `PyAgentCard::prompt`, `PromptCard.ref`, `WyrdState.get`/`__getitem__`; TS `AgentCard`/`PromptCard`/`HydratedCard`, `WyrdState.get` over native `hydrated_agent` | R `register_and_hydrate::hydrated_agent_prompt_card_navigation`; P `test_hydrated_agent_prompt_card_navigation`; T `hydrated agent prompt card navigation` | PASS |
| Python union narrows with `isinstance`; TS union narrows by literal kind | `python/wyrd/state/__init__.pyi` `HydratedCard`; `wyrd/src/index.ts` `HydratedCard` | `tests/typecheck/state_contracts.py` (`mise run py:typecheck`); `tests/types/card-types.test-d.ts` (`mise run ts:typecheck`) | PASS |
| Typed accessors exact; `agent.prompt()` is PromptCard, `.ref` is CardRef; negative type cases | Stubs `cards/__init__.pyi`, `prompt/__init__.pyi`, `state/__init__.pyi`; TS interfaces | Same compile-only suites (`ty: ignore` with unused-ignore errors; `@ts-expect-error`) | PASS |
| `prompt_ref`/`promptRef` identity; `AgentSpec.prompt` unchanged | `HydratedAgent::prompt_ref`, `PyAgentCard.prompt_ref`, TS `promptRef` | R/P/T | PASS |
| Invocation preserved through the shared relationship | `wyrd-client/src/observe/invoke.rs` uses `state.agent(..).prompt()` | I `gateway_inference::run_invokes_its_agent_through_the_gateway` | PASS |
| AC-004 stable errors: unknown alias, kind mismatch, invalid bundle | `TypedCards::agent` load-time invariant | R/P/T negatives; `wyrd-client` lib `agent_with_inline_prompt_is_an_invalid_bundle`, `agent_prompt_rejects_missing_target`, `agent_prompt_rejects_non_prompt_target`; Python `test_agent_with_an_inline_only_prompt_is_an_invalid_bundle` | PASS |
| Consumers, exports, stubs, declarations, examples updated | CLI `card_lifecycle` counts/aliases; Python unit/integration consumers; TS `index.d.ts`/`index.d.cts`; old inline-only Python bundle moved to `bundles/inline-only-agent` | `mise run py:test:unit` (592 passed), CLI `card_lifecycle` (19), `wyrd-client` lib (262), `cargo check -p wyrd-rust-examples`, `mise run ts:napi:check` | PASS |

Commands (all exact selectors above via `scripts/postgres/with-test-postgres.sh` as listed in Verification), plus `mise run fmt`, `lints`, `check:deps`, `codegen:check`, `py:format`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `docs:check`, `git diff --check`: all pass.

Limits: RED was not observed for A before the server change landed; B/R/P/T were written after their implementation, so their proof is GREEN-only. Cross-tenant reads report the existing `WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE`. A Python `AgentCard` not loaded through `WyrdState` raises a validation `WyrdError` from `prompt()`; authored inline prompts stay readable via `prompt_ref.prompt`.

Non-goals confirmed: no other inline-kind change, no standalone Workflow registration, no cascading deletion, no migration, no new Cargo feature, no compatibility alias, no Bifrost/UI change, no external repository edit, `cca791ab6` not used.
