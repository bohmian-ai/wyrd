# Maintainer review

Subject: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5..02c06e21bd5fd613341935a843fabe5350ea0a9d`.

Overall result: **FAIL**.

## Coverage

Reviewed the full cumulative diff against approved specification revision 2 and the original task. Expanded the supplied map with CodeGraph before source searches. Governing guidance: `AGENTS.md` §§2–9, 11, 16; `architecture/agent-rules.md`; spec-driven development; maintainer style; architecture patterns; Rust core; Python API/stubs; TypeScript guide; protocol doctrine 17, Agent/Prompt, registration and reference-form contracts; corresponding public doctrine.

| Changed surface | Source, consumers and proof inspected | Assessment |
| --- | --- | --- |
| Server registration | `RegistrationWriter::write`, `inline_prompt_submission`, complete `persist_node`, graph planning, replay seed and submission lookup; HTTP registration test helpers and `inline_agent_prompt_materializes` | Existing transaction, version-line persistence and lifecycle owners are reused. The helper is invocation-local and does not need a new owner. Rustdoc gap below. |
| Shared hydrated state | `TypedCards::agent`, `HydratedAgent`, `HydratedCard`, `WyrdState::agent/get/prompt`, `hydrate_typed_cards`, state negative tests and aliases | Borrowed Agent view earns its type by preserving a complete Agent/Prompt relationship. Existing index and alias owners remain authoritative; no replacement graph or cache. |
| Invocation and Rust SDK/CLI | Full `Run::invoke`, Rust SDK module re-export, registration/navigation journeys, CLI lifecycle fixture aliases/counts and typed consumers | Invocation uses the same resolved relationship. SDK exports expose the new types through the existing state module. CLI assertions account for the independently registered Prompt. |
| Python holders and hydration | `PyAgentCard::from_native/from_card/attach_prompt_card/prompt`, holder traversal/clearing, `PromptCard::ref_py/as_card_ref_py`, `PythonStateHydrator::hydrate/hydrate_agents`, `PyWyrdState::get/__getitem__` and typed accessors | Existing holder maps and hydration owner are extended; retained Python references participate in the existing lifetime handling. No separate relationship resolver. |
| Python declarations and consumers | Cards/prompt/state authored stubs, runtime public `cards` and `state` package exports, `state_contracts.py`, state/runtime unit tests, registration and state journeys, complete and inline-only fixtures | Exact union/narrowing and PromptCard return types are declared, but the new public alias has no runtime export (MNT-001). |
| TypeScript | Native `agent`/`hydrated_agent`, neighboring typed native accessors, public AgentCard/PromptCard/HydratedCard and state methods, both native declaration diffs, compile-only cases and registration journey | Thin projections preserve typed kind discrimination and expose the exact shared-state relationship. Native declaration changes match the Rust documentation changes. |
| Documentation and shared authoring fixtures | All five `fixtures/cards/agent_prompt` files; protocol/doctrine and docs-site changes | Inline exception is scoped to registered Agents. Shared fixtures cover all three authored forms and explicit alias reuse without a new Service alias namespace. |

Whole-workflow alternatives checked: generated inline Prompt persistence uses the existing `persist_node` path rather than a new registration saga; Agent navigation uses the typed hydrated index rather than the loader, network or registry; Python reuses its pre-existing Prompt holder map; TypeScript only projects the relationship established by shared Rust. No unearned replacement workflow or speculative abstraction found.

## Material findings

### MNT-001 — Public Python union is declared but cannot be imported at runtime

- Classification: **MISSING**; should-fix, public-contract consequence.
- Changed location: `sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi:79`.
- Governing obligation: specification REQ-006 (public typing/exports and importable types); `AGENTS.md` §8; maintainer guide “Python and TypeScript: document the typed contract”; Python API/stubs “Stubs must match runtime imports.”
- Evidence: the stub introduces public `HydratedCard: TypeAlias` and uses it for both lookup returns. Runtime `python/wyrd/state/__init__.py` imports only `CardEnvelope`, `HydratedArtifact`, `WyrdState`, with those same three entries in `__all__`; no runtime definition or re-export supplies `HydratedCard`. Repository search finds the alias only in the stub. The compile-only test imports it inside `if TYPE_CHECKING`, so that proof deliberately never exercises the advertised runtime import.
- Concrete cost: a caller using `from wyrd.state import HydratedCard` for its function annotations passes static checking but fails on module import. Maintainers cannot safely use the package's advertised union in ordinary Python modules or runtime annotation inspection.
- Smallest correction: define this pure-Python alias once in runtime Python, export it through `wyrd.state` and `__all__`, and have the authored stub re-export that definition rather than maintain a second union. Keep the existing five member types and narrowing semantics.
- Focused closure proof: add the ordinary public import to the existing Python contract/export or state test and assert that its union members are the five public holder types. Keep the positive/negative compile-only cases and run `py:lints`/`py:typecheck` with the focused runtime test. No native wrapper or resolver is needed.

### MNT-002 — New HTTP proof helpers omit mandatory panic contracts

- Classification: **VIOLATION**; must-fix under the explicit rustdoc gate.
- Changed locations: `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:4046` (`native_prompt`), `:4057` (`register_inline_agent`), `:4075` (`outcome_for`), `:4085` (`persisted_agent_prompt`), and `:4108` (`inline_agent_prompt_materializes`).
- Governing obligation: `AGENTS.md` §16 requires `# Panics` whenever a panic remains possible, explicitly including new test helpers and test functions; maintainer guide “Documentation: explain what a maintainer cannot infer.” The task-review skill explicitly retains missing/incomplete rustdoc as blocking.
- Evidence: these new items contain `expect`, `panic!`, or assertions, but their rustdoc has no `# Panics` section. For example, `outcome_for` panics both for non-array outcomes and for a missing requested kind; `persisted_agent_prompt` also assumes an accessible UID, successful tenant DB operations and a stored Agent with a Ref-form Prompt. Neighboring existing Rust SDK additions `register_inline`, `persisted_prompt` and both new journeys document their panic assumptions.
- Concrete cost: fixture/response shape and test-environment assumptions remain implicit in reusable helpers, and these additions do not satisfy the repository's documented merge gate even if execution succeeds.
- Smallest correction: add concise, substantive `# Panics` sections to these five changed items naming their actual setup, response-shape and assertion assumptions. Preserve all assertions and behavior; do not create a harness or change errors to avoid documenting them.
- Focused closure proof: inspect the five sections against their complete function bodies and run the owning documentation/format checks. This is documentation-only closure and needs no new runtime scenario.

## Non-blocking calibration

None. No line-count, naming, layout or equally clear alternative has been promoted into a finding.

## Verification limits

Static source review only; no builds or tests were run, as requested for coordinated review. The task records owner checks, focused runtime journeys and static typing/codegen passes; those claims do not substitute for the source/export parity check above. HEAD remained the candidate throughout review. Reviewed source was not modified; only this report was written.

Skipped independent execution of verification lanes; runtime import parity remains a public-contract risk until MNT-001 is closed.
