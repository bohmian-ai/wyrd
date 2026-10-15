# Maintainer review

Overall result: **FAIL** — one bounded continuation of the Rustdoc obligation remains in the newly modified SQL version owner. No optional refactor or additional abstraction is required.

Subject: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5` → `a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`. HEAD verified at candidate. Latest-fix range used for navigation only; cumulative implementation, consumer, fixture, test, declaration and architecture changes reviewed. Source unchanged; this report is the only owned artifact.

## Authority and scope

Read task-review maintainer instructions, maintainer-style, spec revision 2, original task, prior validated ledger and R1 remediation/evidence. Applied AGENTS.md §§3–9, 11, 16; agent rules; active design/doctrine sections governing Agent Prompt materialization, reference forms and hydrated lookup; spec-driven-development; architecture patterns; Rust-core, Python API/stubs and TypeScript guidance. Ponytail and Rust/Python guidance were applied subordinate to repository authority. CodeGraph preceded code searches.

Review scope is maintenance of the approved workflow: complete existing registration/version owner reuse, precise public projections, local read semantics, consumer closure, meaningful test assertions and documentation. Security/data reviewers own independent integrity and tenancy acceptance. The owner-accepted FIND-TASK-001-1 interleaving is not reopened and no lock/recheck/test for it is requested.

## Changed-surface coverage

| Changed surface | Full symbols, module context and connected consumers reviewed | Assessment |
|---|---|---|
| `crates/wyrd/wyrd-server/src/components/cards/service.rs` | `register_card`, `RegistrationWriter::write`, `persist_node`, `resolve_existing`, `inline_prompt_submission`, `pin_existing_inline_prompt`, seed/outcome construction and canonical reference binding | Materialization remains inside existing transaction/lifecycle. Stateless conversion stays beside its caller; exact-parent normalization delegates canonical comparison to existing persistence. No second graph, registry owner, resolver, transport or SDK registration path. |
| `crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs` | Complete module, `SubmittedCardIdentity`, `latest_stable_in_line`, `resolve_version`; all production call sites and re-export | Current-operation identity is threaded through the sole registration caller, across auto and scoped version modes. Ordinary authored nodes and generated nodes use the same selection. Missing docs below. |
| `crates/shared/wyrd-client/src/state.rs` | Existing typed maps and alias index; `TypedCards::agent`, complete `HydratedAgent` and `HydratedCard`, `WyrdState::agent/get`, complete typed hydration pass; removed resolver/error helpers and changed tests | Borrowed view earns its type by representing a complete Agent/Prompt relationship. State construction verifies that relationship using existing maps. Closed union preserves typed accessors and fallback envelopes. No client or IO added to pure Card specs. |
| `crates/shared/wyrd-client/src/observe/invoke.rs` | Complete `Run::invoke`, gateway/runtime preparation and error helpers | Invocation reads the same selected Prompt holder; it retains existing tool restriction, run correlation and gateway flow. |
| `crates/wyrd/wyrd-cards/src/agent.rs`, `prompt.rs` | `PyAgentCard` storage/from-native/from-card/attach/prompt, construction and GC lifecycle; Prompt reference accessor alongside `as_card_ref_py` | Existing migration wrapper is extended in place. Attached holder has meaningful lifecycle and is shared with state. Native Prompt capabilities remain accessible through PromptCard. Field lifecycle docs now explain unattached and GC-cleared refusal. |
| Python state native projection | `src/state/hydrator.rs::hydrate`, `src/state/mod.rs::get/__getitem__/hydrate_agents`; existing prompt construction, alias lookup, cached holders, envelope fallbacks | Prompts are built once, then existing Agent holders attach selected Prompt holders. Binding dispatch projects shared validated kind and existing caches, rather than creating a language-specific graph resolver. |
| Python runtime/public typing | `cards/__init__.pyi`, `prompt/__init__.pyi`, `state/__init__.py`, `state/__init__.pyi`, `_hydrated.py`; surrounding public package imports | Five union members match runtime dispatch; one inline Python union definition is re-exported, rather than duplicated in a stub. Agent prompt return and Prompt ref match runtime methods. Public runtime import independently passes. |
| TS native and declarations | `native/src/cards.rs::NativeWyrdState::agent`, complete `hydrated_agent`, existing state-read and envelope APIs; `index.d.ts`, `index.d.cts` changed declaration docs | Foreign boundary serializes the shared relationship and stored envelopes; it does not select versions or resolve another graph. Declaration shape remains the existing lifecycle-result transport. |
| TS public wrapper | `wyrd/src/index.ts::AgentCard/PromptCard/HydratedCard`, complete `WyrdState::get/agent/prompt`, existing card/ref/workflow methods | Exact typed navigation and discriminated union replace broad envelopes only where required. Wrapper adds local methods/ref fields to shared results. No unchecked caller-selected Card kind or `any` introduced. |
| HTTP regression proof | Complete appended `pg_card_registration_route.rs` helpers and three tests, including R1 exact-pin and current-operation selection cases | Tests assert persisted exact pins, outcomes, row/operation counts, sibling edges, drift rollback and both same-content/different-content selection. Existing real-server test home reused; no added harness. |
| Rust SDK and CLI proof | Complete new `register_and_hydrate.rs` helpers/journeys, changed existing CLI lifecycle assertions and nearby fixture setup | Public client registration/hydration, explicit aliases, exact identity, tenant isolation, actual shutdown followed by local reads. Existing CLI counts reflect one additional normal Prompt node. |
| Python tests/fixtures | `tests/typecheck/state_contracts.py`; changed hydration, runtime Agent, state and registration journeys; complete and inline-only bundle changes | Positive and negative typing uses public imports; runtime import checks the union. Runtime Prompt consumers updated to PromptCard method or authored prompt_ref. Old inline-only fixture is preserved separately for rejection proof. |
| TS tests | Changed registration/hydration journey and `card-types.test-d.ts`, surrounding type-test imports | Public union narrowing, exact navigation/ref types and rejected unnarrowed assignments are asserted separately from runtime identities/payload. |
| Shared authored fixtures | All five `fixtures/cards/agent_prompt/*.yaml` | Existing loader full envelopes, path/ref identities, inline native body, Service aliases and explicit Prompt alias exercised without new loader or bundle format. |
| Durable docs and active packet | Changed design/doctrine and public Cards/versions guide; original task additions and supplied R1 evidence | Scope of inline exception and re-register/rehydrate rule is documented; unrelated inline semantics preserved. Historical review files are inputs, not production obligations. No current-round peer report or implementation completion summary was read. |

## Material finding

### MNT-R2-001 — VIOLATION — continue FIND-TASK-001-5: changed SQL selection lacks required function contracts

- **Changed locations:** `crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs:49` (`latest_stable_in_line`) and `:101` (`resolve_version`, panic branch at `:112–114`).
- **Authority:** AGENTS.md §16 requires substantive rustdoc on every materially modified Rust item regardless of visibility, `# Errors` on fallible functions, and `# Panics` whenever a panic remains possible. Missing Rustdoc is explicitly a hard blocker. Maintainer-style requires documentation of behavior a maintainer cannot safely infer from the signature.
- **Source evidence:** The selector now accepts `operation_id` and changes eligible rows from only Active to Active plus that operation's Pending rows. Its only preceding text is an ordinary `//` statement-timeout comment; it has no Rustdoc and no error contract. The public resolver now consumes the operation-aware identity but still panics for `Some(VersionSpec::Pin(_))`, with no `# Panics` section. Its sole production caller currently excludes Pin through `resolve_existing`, which makes the invariant explainable rather than removing the public function's panic condition.
- **Concrete maintenance cost:** This remediation changes the eligibility boundary central to avoiding duplicate allocation. Generated API docs do not tell maintainers the selector's operation-local visibility/error behavior, and the public resolver's documented error contract omits the exact-pin precondition. A future caller can incorrectly treat a valid typed Pin as fallible input while receiving a panic.
- **Smallest correction:** Add substantive Rustdoc to the existing selector describing stable latest-in-line selection, Active/current-operation Pending eligibility, caller-owned transaction and its bounds/database/stored-version errors. Add the public resolver's exact-pin panic precondition. Preserve signatures, SQL, locking, lifecycle and tests; do not introduce validation machinery or refactor the selector for this finding.
- **Focused closure proof:** Static comparison of docs with these two complete bodies and the sole production caller, then existing applicable format/documentation checks. No new behavioral test or harness is needed for prose.

This is the same Rustdoc cause as prior FIND-TASK-001-5, expanded only to the new remediation's materially changed SQL symbols. The originally cited field and six HTTP items are closed; this finding does not reopen them.

## Prior finding disposition

| Prior ID | Source assessment |
|---|---|
| FIND-TASK-001-1 | Owner-accepted risk; excluded as instructed. No mechanism or test prescribed. |
| FIND-TASK-001-2 | Closed in maintainer scope: `pin_existing_inline_prompt` finds the exact stored Agent, preserves generated kind/name/space, and routes the selected Prompt pin back through canonical persistence/drift checks. Exact reapply test also checks changed Prompt/non-Prompt rejection and sibling Service binding. |
| FIND-TASK-001-3 | Closed in maintainer scope: operation identity enters existing SQL latest-line owner for both auto and scope modes. Regression covers two Agent versions, matching authored Prompt@0.1.0, different bodies and conflicting authored content. Outcomes suppress repeated selected generated UID without replacing registration ownership. |
| FIND-TASK-001-4 | Closed: single public Python union definition and runtime/stub exports agree; independent no-sync import confirms its five exact public members. Positive/negative type contracts remain. |
| FIND-TASK-001-5 | Original field/six HTTP doc omissions closed. Continue only for newly modified SQL symbols in MNT-R2-001 above. |

## Verification and limits

Independently ran `PYTHONDONTWRITEBYTECODE=1 mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -c ...` to import public HydratedCard and assert its five public union members: PASS. No source/dependency mutation or extension rebuild was performed.

R1 appended evidence records exact focused reapply/current-operation tests, original materialization and Rust identity regression, all HTTP registration tests, Python state tests, type/lint/format/dependency/tenancy checks as passing. Those results are supporting evidence; they were not rerun here. Source inspection establishes the documentation omission despite green mechanical checks.

No optional structural/preferences findings. The views, closed unions and existing owner extensions have actual callers and avoid speculative layers.

Skipped Cargo builds, live Postgres reproduction, broad suites and unrelated cleanup. Residual risk: owner-accepted deletion interleaving remains; runtime/integration results beyond the import check rely on recorded evidence and other review roles.
