# Repository standards review

Overall: **FAIL** — one rustdoc hard-gate finding. Source and verification were inspected independently; no other reviewer report was read.

## Immutable subject and method

- Base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`.
- Candidate / checked HEAD: `02c06e21bd5fd613341935a843fabe5350ea0a9d`.
- Scope: all 37 paths in the cumulative diff; repository-standards role only.
- CodeGraph was used first to locate the registration writer, typed state, and Python hydration owners, then the complete diff and surrounding source/consumers were inspected. No source was edited and no builds or tests were run.
- Approved specification: `changes/active/agent-prompt/spec.md`, revision 2. Original task's appended verification claims were inspected as claims, not accepted as source proof.

## Authority coverage

The reference router is `architecture/references/README.md`. All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx`. Focused references below are under `architecture/references/`.

| Changed surface / complete path coverage | Applicable additional authority | Source/consumer coverage |
|---|---|---|
| `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `docs/src/content/docs/understand/cards-versions-and-relationships.md` | `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/agent-harness.md` | Inline exception, Agent contract, reference forms, independent Card identity, user documentation; unchanged Workflow boundary compared. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs` | `architecture/wyrd-security-posture.md` (trust, authorization, tenant data, audit); `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/agent-harness.md` | `register_card`, `RegistrationWriter::write`, `persist_node`, `inline_prompt_submission`, version locks, replay seed, caller attribution, tenant transaction and registration lifecycle. |
| `crates/shared/wyrd-client/src/state.rs`, `crates/shared/wyrd-client/src/observe/invoke.rs` | `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md` | `TypedCards`, `HydratedAgent`, `HydratedCard`, generic/typed lookup, `hydrate_typed_cards`, removed bare-Prompt resolver, `Run::invoke`, Rust SDK state re-export. |
| `crates/wyrd/wyrd-cards/src/agent.rs`, `crates/wyrd/wyrd-cards/src/prompt.rs` | `languages/rust-core.md`, `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, `languages/errors.md` | Retained owner-crate Python feature, holder fields, construction, attachment, prompt method/ref getter, traversal/clear, registration in `wyrd-cards/src/lib.rs`. |
| `sdks/wyrd-sdk-python/src/state/mod.rs`, `sdks/wyrd-sdk-python/src/state/hydrator.rs` | Same Python/Rust/error references; `architecture/patterns.md` | All-or-nothing holder hydration, prompt-before-agent construction, shared-state relationship resolution, cached holders, get/indexing, native registration and SDK entry point. |
| `sdks/wyrd-sdk-python/python/wyrd/{cards,prompt,state}/__init__.pyi` | `languages/python-api-and-stubs.md`, `languages/maintainer-style.md`, `languages/pyo3-boundaries.md` | Native method/property parity and public package exports; exact PromptCard/CardRef return types; generic union and narrowing. |
| `sdks/wyrd-sdk-ts/native/src/cards.rs`, `sdks/wyrd-sdk-ts/wyrd/src/index.ts`, `sdks/wyrd-sdk-ts/wyrd/index.d.ts`, `sdks/wyrd-sdk-ts/wyrd/index.d.cts` | `languages/typescript-guide.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/maintainer-style.md` | Native shared-state lookup and JSON projection, public hydrated interfaces and discriminated union, typed get/agent/prompt methods, source/generated declaration correspondence. |
| `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs`, `sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs`, `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs` | `TESTING.md`, `languages/testing-workflows.md`, `languages/rust-core.md`, `languages/maintainer-style.md` | New HTTP/Rust journeys and helpers, SQL assertion capability, negative registration and lookup assertions, CLI counts/aliases, integration module registration. |
| `sdks/wyrd-sdk-python/tests/integration/state/test_state_journey.py`, `tests/integration/test_register_and_hydrate.py`, `tests/unit/runtime/agent/test_agent_journey.py`, `tests/unit/state/test_hydration.py`, `tests/typecheck/state_contracts.py` (all under Python SDK) | `TESTING.md`, `languages/testing-workflows.md`, `languages/python-api-and-stubs.md`, `languages/pyo3-boundaries.md` | Public imports, top-level tests, runtime versus typing ownership, real server journeys, invalid old bundle proof, positive/negative typing fixtures. |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts`, `tests/types/card-types.test-d.ts` | `TESTING.md`, `languages/testing-workflows.md`, `languages/typescript-guide.md` | Real native/server journey, unknown/kind refusal assertions, exact typed navigation, discriminant narrowing and expected compiler errors. |
| `fixtures/cards/agent_prompt/{desk-prompt,inline-agent,path-agent,prompt-desk,ref-agent}.yaml`; Python `tests/fixtures/bundles/complete/cards/inline_agent/{card,relationships}.yaml`; Python `tests/fixtures/bundles/inline-only-agent/cards/inline_agent/{card,relationships}.yaml` | Protocol authoring/identity rules; `languages/testing-workflows.md` | Shared inline/path/ref authoring inputs, explicit Prompt alias, existing valid bundle now pins registered Prompt; original inline-only envelope preserved for rejection test. |
| `changes/active/agent-prompt/tasks/TASK-001-agent-prompt.md` | `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/testing-workflows.md` | Appended focused commands/results, stated GREEN-only history and recorded verification scope. |

Manifests inspected: workspace Rust edition/MSRV/PyO3/N-API versions; `wyrd-client`, `wyrd-cards`, Rust SDK, Python SDK and TS native Cargo manifests; Python typecheck configuration; TS typecheck configuration; relevant `mise.toml` build/check/test bodies. No dependency, Cargo-feature, migration, build-system or UI changes exist in the diff. Bifrost/OLAP/Arrow-execution and operations references are not governing a materially changed path here: the change does not alter those owners or their deployment/recovery contracts.

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Server owns durable registration/versioning/relationships; Cards remain declarative (`AGENTS` §§2–3,9) | PASS | `service.rs:1127–1136` materializes the Prompt through existing `persist_node`; client `HydratedAgent` only borrows already-loaded values. |
| One tenant transaction; actual SQL capability types; callee cannot commit (`agent-rules`) | PASS | `RegistrationWriter::write` acquires `registry_tenant_conn(self.caller.data_tenant_id)`, passes `&mut TenantConn<'_>` to `persist_node` (`service.rs:1179`), and commits at the existing writer boundary. Generated child uses that same connection. No raw pool, naked transaction or manual tenant filter was introduced. Test helper `persisted_agent_prompt` opens and owns its connection before committing. |
| Auth/audit remain server-owned and non-blocking (`AGENTS` §9; security posture) | PASS, static scope | New branch receives the existing authenticated `Caller` and attribution; no alternate permission check, credential, audit writer or transaction-coupled audit is added. HTTP test asserts viewer denial and no registration writes. This is not an independent runtime security certification. |
| Struct-centered ownership and narrow async (`AGENTS` §§5–6) | PASS | Existing `RegistrationWriter`, `WyrdState`, `TypedCards`, and `PythonStateHydrator` retain their responsibilities. New `HydratedAgent` owns a coherent borrowed relationship invariant. Pure inline submission and native projection helpers are synchronous; new async test helpers await real IO. |
| Typed identity, stable derived errors, no unsafe/external-input unwrap (`AGENTS` §4) | PASS | New name goes through `CardName::new`, generated identity comes from `persist_node`, errors use existing `WyrdError` variants; changed production paths add no unwrap, unsafe, ad hoc runtime or hand-written error catalog. |
| Rustdoc for every materially changed item, errors and panics (`AGENTS` §16) | **FAIL** | `STD-001` below: changed holder field lacks rustdoc and six new server-test items omit panic documentation. |
| PyO3 lifetime, manual nested getters, Python aggregation (`AGENTS` §§7–8) | PASS for lifetime/aggregation | New retained objects use `Py<T>` and `clone_ref`; no `Bound` crosses await. Prompt Card and Agent Card remain registered through Python SDK bootstrap. Native Prompt is projected through the existing PromptCard hydration owner. |
| New PyO3 belongs in Python SDK (`AGENTS` §§2,7; routed PyO3 placement) | FAIL, non-blocking placement note | The new `PromptCard::ref_py` getter is added at `crates/wyrd/wyrd-cards/src/prompt.rs:676`; changed Agent attachment API also remains in retained owner-crate adapter (`agent.rs:81`). Retaining existing wrappers is allowed, but the rule does not expressly authorize adding new Python behavior there. No runtime, security or contract defect follows solely from this placement; under task-review severity calibration it does not independently force the overall FAIL. |
| Python stubs/public imports/precise typed returns (`AGENTS` §8) | PASS, static | Agent method returns PromptCard, Prompt ref returns CardRef, get/indexing return explicit union; native public classes are already exported. Compile-only fixture imports `HydratedCard` under `TYPE_CHECKING` and checks narrowing and invalid assignments. `ty` includes tests and rejects unused ignores (`pyproject.toml:105–118`). |
| TypeScript thin projection and sound public union (`typescript-guide`) | PASS, static | Native resolves via shared `WyrdState::agent`; TS receives envelope plus exact reference and projects typed navigation. `HydratedCard` excludes plain Agent/Prompt envelopes before adding hydrated variants. Type tests exercise literal-kind narrowing and incompatible operations. |
| Generated declarations source-driven (`agent-rules`) | PASS for inspected correspondence; execution unverified | Only matching native Agent rustdoc changes appear in `index.d.ts`/`.d.cts`. `mise.toml:1483–1504` identifies `ts:build` and `ts:napi:check`; no source evidence indicates hand editing. I did not regenerate declarations. |
| Journey/integration/unit boundaries and test runtime ownership (`AGENTS` §11) | PASS for added test architecture | Rust public SDK and HTTP tests use real WyrdTestServer/Postgres ownership; Python/TS lifetime behavior runs in owning runtimes. State malformed-bundle tests remain local. Python tests are top-level functions and public imports are used. |
| Test integrity; no weakened gates or unrelated suppressions (`AGENTS` §12) | PASS in diff | Updated counts account for new registered Prompt. Old inline acceptance is intentionally replaced by approved invalid-bundle behavior, with a dedicated preserved negative fixture. New ignored Rust journeys carry repository-managed Postgres reason; they are explicitly selected with `--run-ignored=all`. Compiler-negative `ty: ignore` / `@ts-expect-error` lines are test assertions, not production suppressions. |
| Required Red-Green execution record (`spec-driven-development`) | FAIL as recorded process limitation, non-blocking here | Task evidence expressly says A had no observed RED and B/R/P/T were added after implementation. No failing gate was suppressed to achieve this change. This cannot be repaired by inventing retrospective RED; it does not itself prove a source behavior defect. |
| Scope, vocabulary, protocol/docs alignment (`AGENTS` §§2,12,14) | PASS in changed text | Protocol/doctrine and docs all describe the scoped Agent-inline Prompt exception. No removed Card kind, compatibility alias, destructive migration, feature, external repository or unrelated UI/Bifrost behavior was added. |
| Required scoped verification (`AGENTS` §§11–12) | Recorded claims inspected, not independently executed | Appended evidence names focused HTTP/Rust/Python/TS journeys, invocation regression, typing, codegen, native declarations, lints, docs, and boundaries. Commands correspond to owning mise tooling. Full affected journeys are explicitly deferred to integrated change review. |

## Material source-local findings

### STD-001 — VIOLATION — Complete rustdoc for the changed holder and new server test items

**Authority:** `AGENTS.md` §16 requires rustdoc on every new/materially modified Rust item, explicitly including fields, private helpers and tests, and `# Panics` wherever panic remains possible. `architecture/agent-rules.md` calls missing/incomplete rustdoc `BLOCK_BEFORE_MERGE`; task-review expressly preserves this as blocking even without a behavioral defect.

**Exact changed locations:**

- `crates/wyrd/wyrd-cards/src/agent.rs:33`: `PyAgentCard.prompt` changes from a runtime Prompt to an attached registered PromptCard but has no field rustdoc. The field's meaning and `None` lifecycle are central to the new public `prompt()` behavior.
- `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:4029`: new `inline_agent_request` calls the panicking native-prompt builder, with no `# Panics`.
- Same file `:4046`: new `native_prompt` contains two `expect` calls, with no `# Panics`.
- Same file `:4057`: new `register_inline_agent` contains `expect` and `assert_eq!`, with no `# Panics`.
- Same file `:4075`: new `outcome_for` contains `expect` and explicit `panic!`, with no `# Panics`.
- Same file `:4085`: new `persisted_agent_prompt` contains connection/read/commit `expect` calls and kind/reference `panic!` branches, with no `# Panics`.
- Same file `:4108`: new `inline_agent_prompt_materializes` has setup and semantic assertions/panics, with no `# Panics`.

**Consequence:** The change does not satisfy the repository's explicit documentation acceptance gate. A maintainer cannot obtain the new attachment lifecycle or test-helper failure contract from the item documentation. This is a documentation blocker, not a claimed production panic bug.

**Smallest testable correction:** Document the changed PromptCard field's unattached/hydrated/cleared lifecycle and add substantive `# Panics` sections describing fixture construction, server/SQL failures and failed assertions on the six new server-test functions. Keep the test bodies, assertions and runtime semantics unchanged. Inspect all touched items together so the correction closes this single documentation gap; do not expand into unrelated existing wrapper documentation.

**Closure proof:** Static inspection of the field and all six function docs against §16; applicable formatting/rustdoc checks for the bounded documentation write set. No new runtime test is needed to prove prose additions.

## Verification notes and residual limits

No builds, tests, codegen, or network standards lookups were run. No version-specific external API assumption is needed for the finding. The task's verification claims were compared with source and task definitions but were not independently reproduced. Python/TypeScript offline journeys switch the configured URL to an unreachable address while retaining their running harness; only the Rust journey physically shuts its server down before loading. The independent behavior reviewers own acceptance assessment of that distinction.

Skipped unrelated pre-existing debt and all implementation changes. Residual risk is runtime proof not reproduced by this static standards pass; source-level rustdoc noncompliance is confirmed independently of green checks.
