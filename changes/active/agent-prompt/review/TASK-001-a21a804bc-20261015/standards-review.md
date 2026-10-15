# Independent repository standards review

Result: **FAIL**

## Subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt`.
Complete reviewed range: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5..a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`.
Latest remediation navigation: `02c06e21bd5fd613341935a843fabe5350ea0a9d..a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`.
HEAD checked at assigned candidate. Source was not edited; this report is the only owned output. No current-round reviewer reports were read, and no subagents or Cargo commands were launched.

Inputs: immutable subject, approved spec revision 2, original task, remediation diagnoses and available verification evidence. Owner acceptance of FIND-TASK-001-1 remains accepted; this audit does not reintroduce its lock/recheck/concurrency-test requirement. CodeGraph was attempted before source navigation; its broad query returned unrelated symbols, so cumulative diff/direct source inspection supplied the missing navigation. Every product/test/language surface in the complete diff was reviewed, including changed/new Rust helpers and their complete bodies.

## Material finding

### STD-001 — FIND-TASK-001-5 remains incomplete in the newly changed SQL owner

- Classification: **VIOLATION**, repository rustdoc gate; blocking under AGENTS.md §16 and the task-review skill's explicit rustdoc exception.
- Locations: `crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs:49` (`latest_stable_in_line`) and `:101` (`resolve_version`, panic at `:113`).
- Rule: every new or materially modified Rust item, including private helpers, needs substantive rustdoc; every fallible function needs `# Errors`, and remaining panics need `# Panics`. Agent rules repeat this as `BLOCK_BEFORE_MERGE`; Rust core Documentation requires workflow role, invariants and applicable cancellation/partial progress.
- Evidence: both cumulative and latest diff materially change `latest_stable_in_line`'s parameters and query to include this operation's Pending rows. Its only preceding prose is an ordinary `//` timeout comment, not rustdoc; eligibility/transaction role and bounds, stored-version parsing and database errors have no rustdoc contract. `resolve_version` is also materially changed to use that eligibility. Its `# Errors` exists, but public `version` still accepts `Some(VersionSpec::Pin(_))`, which executes `unreachable!`; no `# Panics` explains the auto/scope-only invariant. The sole production caller at `service.rs:1429-1464` dispatches exact pins separately, so this is a documentation finding, not a claim that current registration reaches that panic.
- Consequence: the newly modified version-selection seam fails the explicit merge gate and leaves current-operation eligibility and a public panic precondition undocumented. The repaired Python field and original six HTTP items do not substitute for documenting newly modified helpers.
- Smallest correction: add substantive rustdoc and `# Errors` to `latest_stable_in_line`, covering Active/current-operation Pending stable-row eligibility, optional bounds, caller-owned transaction and read-only/error behavior. Add `# Panics` to `resolve_version` naming the exact-pin precondition. Preserve behavior and the caller split; no abstraction, new error, runtime test or broad cleanup is needed.
- Closure proof: inspect complete bodies against their docs and run the already-required scoped format/lint proof. Do not add optional workspace `cargo doc -D warnings` as a new required lane.

No additional material standards findings established.

## Authority coverage

Common authority for every row: `AGENTS.md`, `architecture/agent-rules.md`, reference router `architecture/references/README.md`, and task-review repository-standards instructions. Selected focused references were read completely. Design inspection covered Doctrine/client model/kinds/Prompt/Agent/Workflow/Service, registry lifecycle, reference forms/inventory/path rules; security inspection covered applicable identity, tenant, authorization and audit boundaries.

| Changed surface (complete cumulative scope) | Additional applicable authority | Coverage/result |
|---|---|---|
| `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, docs page `understand/cards-versions-and-relationships.md` | Active protocol/doctrine; `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`; AGENTS §§2,9,14 | Scoped Agent-inline-Prompt exception, derived identity, navigation and old-bundle refusal agree with approved direction; other inline semantics preserved. PASS. |
| Original task and historical `review/TASK-001-02c06e21b-20261015/*.md` artifacts | `languages/spec-driven-development.md`, `languages/implementation-execution.md`; AGENTS §§11,14 | Packet/history/evidence, not stronger product authority. Missing original RED disclosed; remediation RED/GREEN recorded. Owner waiver explicit. PASS for evidence handling. |
| Server `components/cards/service.rs`; SQL `queries/cards/version_resolve.rs` | `architecture/patterns.md`; Rust core/errors/agent harness; security posture; active registry/reference authority; AGENTS §§3–6,9,15–16 | Writer/materialization/pin reuse/current-operation eligibility, persistence/activation callers, actual connection types inspected. Rustdoc FAIL; remaining inspected boundaries PASS. |
| Shared `wyrd-client/src/state.rs`, `observe/invoke.rs` | Architecture patterns; Rust core/errors; active client/Agent/Service contract; AGENTS §§3–6,9,16 | TypedCards, borrowed Agent/Card views, get/agent dispatch, load-time target validation, invocation and changed unit tests inspected. PASS. |
| `wyrd-cards/src/agent.rs`, `prompt.rs`; Python SDK `src/state/mod.rs`, `hydrator.rs` | `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, Rust core/errors; AGENTS §§7–8,16 | Existing feature-gated migration wrappers, GC lifecycle, manual Prompt access, ref getter, SDK hydrator and native registrations/imports inspected. PASS. |
| Python state runtime/stub/new `_hydrated.py`; cards/prompt stubs | Python API/stubs; maintainer style/errors; AGENTS §8 | Single union definition; matching runtime/stub reexports and __all__; five exact public members; native dispatch; navigation types. Independent public import proof passes. PASS. |
| TS native `cards.rs`, wrapper `src/index.ts`, `index.d.ts`, `index.d.cts` | `languages/typescript-guide.md`; errors/maintainer style; AGENTS §§2–3,9,16; generated-artifact rules | Shared-state lookup/JSON boundary; literal-kind union and exact navigation; package declaration routing; generated native docs match Rust source. PASS by source and recorded ts:napi:check; not independently regenerated. |
| Rust SDK registration/hydration integration tests; CLI lifecycle tests; server registration-route tests | `languages/testing-workflows.md`, Rust core/maintainer style/implementation execution; AGENTS §§11–12,16 | Real-server/database external targets earn placement. New helpers/constants/tests and materially changed test docs inspected; original HTTP helpers now document actual panic assumptions. Additional Prompt counts/navigation assertions coherent. PASS. |
| Python integration/state/runtime/unit/typecheck tests and bundle fixtures | Testing workflows/Python API/stubs/maintainer style; AGENTS §§8,11,16 | Public imports, top-level pytest functions, old-bundle negative fixture, static positive/negative typing, runtime union test. PASS. |
| TS integration/type tests; `fixtures/cards/agent_prompt/*.yaml` | Testing workflows/TS guide; design envelope/reference/path authority; AGENTS §§9,11 | Real registration/hydration, authoring forms/explicit alias, unknown/mismatch errors, discriminated static checks; ordinary YAML envelope metadata/refs. PASS. |

No UI, new transport, cloud/object-store engine, Bifrost, migration, dependency or Cargo-feature change appears. Analytical domain references do not govern a changed surface here. No external URL fetch was introduced, so no new SSRF implementation obligation is triggered.

## Applicable rule results

| Rule | Source evidence | Result |
|---|---|---|
| Wyrd vocabulary/declarative contracts/server ownership (AGENTS §§2–3,9) | Materialization/versioning remain RegistrationWriter/persist_node; local state views retain references, no client/write. Pure wyrd-spec untouched. | PASS |
| Shared client and no duplicate graph/transport/loader (AGENTS §§2–3,5) | TypedCards lookup; Python attaches prebuilt Prompt holders; N-API projects shared relationship; existing alias/graph owner reused. | PASS |
| Meaningful owners/free-function exception (AGENTS §5) | WyrdState/TypedCards retain index; PythonStateHydrator retains graph/config. Small submission/pin helpers use caller-owned invocation/transaction, no retained lifecycle requiring another owner. | PASS |
| Actual SQL capability (agent rules) | Changed production SQL/helpers take `&mut TenantConn<'_>`; no raw PgPool/PgConnection/Transaction parameter/field or operator escape hatch. | PASS |
| Transaction lifecycle/RLS (agent rules/security posture) | Writer acquires/commits its own connection; borrowed-connection helpers never commit/rollback. Caller tenant supplies registry_tenant_conn; SQL has no manual tenant predicate. | PASS |
| Typed IDs/input validation/SQL injection (AGENTS §§4,9) | CardName/SpaceName/RegistrationOperationId/CardUid retained; new values use push_bind; derived name validated with CardName. | PASS |
| Atomicity and operation visibility (registry contract) | Generated Prompt uses persist_node inside existing transaction before parent hashing; Pending eligibility names current operation; normal activation remains post-commit. | PASS for inspected standard boundary; full interleavings belong to domain review |
| Authorization/audit/trace (AGENTS §9/agent rules) | Existing permission/outbox path remains; generated persistence reuses caller attribution/instrumented writer; no parallel audit/permission writer. | PASS |
| Errors/unwrap/async (AGENTS §§4,6) | WyrdError catalog constructors preserved; new lookup/parsing failures propagated; new async functions await SQL IO; local access synchronous. | PASS |
| PyO3 feature/placement/GIL/lifetimes (AGENTS §7) | Existing migration wrappers remain gated; SDK owns hydration; retained objects are Py<T>, clone_ref shares them; no Bound across await or new runtime. | PASS |
| Python native registration/import/stub parity (AGENTS §8) | Aggregator calls wyrd_cards::register; existing classes register in cards submodules; public Agent/Prompt/state imports and stubs match methods/ref. | PASS |
| Pure-Python single definition/export parity | `_hydrated.py` defines union once; runtime/stub reexport, __all__ match; independent five-member runtime test passes. | PASS; FIND-4 closed |
| Exact Python/TS public types and narrowing | Python static contract uses exact types/isinstance and negative suppressions with unused-ignore errors; TS literal kinds/negative @ts-expect-error; ty/tsc include these files. | PASS by source and available lane evidence |
| Generated declaration parity | d.ts/d.cts changes mirror NativeWyrdState::agent docs; recorded ts:napi:check passes; no wire/schema generator changed. | PASS with regeneration limit |
| Test runtime/tier ownership (AGENTS §11) | Rust tests use Rust runtime/Postgres/server; Python lifetimes in pytest; Node in Vitest. HTTP/all-SDK journeys present; malformed local bundle negative appropriately unit-only. | PASS for standards coverage; integrated suites belong to change review |
| No weakened gates/tests (AGENTS §12) | No new suppression/dependency/feature/gate or ignored regression. New journey ignores use existing gated convention. Old inline-positive fixture becomes explicit negative test. | PASS |
| Rustdoc (AGENTS §16/agent rules) | Original attachment field and six HTTP items corrected; materially modified SQL helper has no rustdoc and public resolver omits panic contract. | FAIL — STD-001/FIND-5 |
| Scope-correct verification/extra optional command | Required recorded remediation fmt/lint/Python/boundary/focused commands pass. Optional strict cargo doc failure distinguished from repository lanes. | PASS for supplied required proof, not independent reruns |

## Prior dispositions

- FIND-TASK-001-1: accepted risk; no new lock/recheck/concurrency proof requested.
- FIND-TASK-001-2/-3: fixes reuse existing exact/version/transaction owner and typed SQL capability. Full behavioral closure remains with implementation/domain reviewers.
- FIND-TASK-001-4: closed independently by source and runtime import/member test.
- FIND-TASK-001-5: original field and six HTTP item docs corrected; cumulative closure fails because remediation materially changed SQL helpers without satisfying the same rustdoc rule (STD-001).

## Verification notes

Independently executed: `mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_hydration.py::test_hydrated_card_is_public_runtime_type` — **1 passed in 0.05s**. No dependency sync or native rebuild.

Available evidence records exact pin/current-operation HTTP RED/GREEN, original registration regressions, all 30 route tests, Python state tests, fmt/lints/py:format/py:lints/py:typecheck/check:deps/check:tenant-isolation and diff check passing. Original evidence also records codegen, TS typing/native declaration and docs checks. These are supplied results, not independent reruns. No broad journey suite is newly required in this task remediation; integrated change review owns complete suites.

The extra `cargo doc` command with `-D warnings` failed on private intra-doc links outside changed files; it is not a required repository lane and is not a separate required-gate finding. The source rustdoc failure above stands independently of that command. No baseline checkout/testing or blame attribution was performed.

Manifests/tooling inspected: workspace Cargo.toml/Cargo.lock (Rust 1.94, edition 2024, PyO3 0.28.3, N-API 3), touched-owner manifests, Python pyproject/typing scope (Python >=3.10), TS package/tsconfig/declaration routing, and relevant mise task definitions. No Cargo command was run under the shared-target coordination instruction.

Skipped independent Cargo/lint/native-generation and full journey reruns. Residual owner-accepted concurrent deletion risk remains as recorded in the subject.
