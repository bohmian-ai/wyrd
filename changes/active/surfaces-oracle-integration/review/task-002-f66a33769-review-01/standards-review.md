# Repository Standards Review — TASK-002

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Candidate `HEAD` before and after inspection: `f66a337698940920dca20b126c1c6c28a6390191`
- CodeGraph: not used because the repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage |
|---|---|---|
| Workspace membership, crate consolidation, shared Rust client, Cards, storage, and `WyrdState` | `AGENTS.md` §§2–6, 9, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model and Registry lifecycle; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/languages/rust-core.md` | Complete |
| Python SDK relocation, PyO3 aggregation, public imports, stubs, and tests | `AGENTS.md` §§2–8, 11–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Rust SDK package and journey | `AGENTS.md` §§2–6, 11–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| TypeScript SDK, napi binding, generated declarations, error-code union, and journeys | `AGENTS.md` §§2, 3, 8–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Bifrost HTTP table routes, OpenAPI, public errors, MCP/CLI consumers | `AGENTS.md` §§2, 9–12; `architecture/wyrd-design.md` Bifrost; `architecture/bifrost-design.md` Public surface; `architecture/wyrd-security-posture.md`; `architecture/references/domain/vala-architecture.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete |
| Docs, examples, generated artifacts, checks, `mise` lanes, and CI routing | `AGENTS.md` §§1, 11–12, 14–16; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Complete |
| Release workflow path relocation | `AGENTS.md` §§2–3, 12, 15; `architecture/operations/deployment-and-release.md` | Complete |

## Applicable-rule results

| Rule or authority | Result | Source and verification evidence |
|---|---|---|
| `wyrd-client` is the sole shared Rust client and the three first-class SDK roots project it | PASS | `crates/shared/wyrd-client/src/lib.rs:1-24`; `sdks/wyrd-sdk-rust/src/lib.rs:1-25`; `sdks/wyrd-sdk-python/Cargo.toml:21-47`; `sdks/wyrd-sdk-ts/native/Cargo.toml`; recorded `check:client-tier` PASS |
| Client-tier dependency cones exclude SQL, server, Vala engine, cloud SDK, and PyO3 from Rust/TypeScript | PASS | `crates/shared/wyrd-client/Cargo.toml:17-58`; `scripts/checks/client-tier.sh`; recorded `check:client-tier` and `check:pyo3-scope` PASS |
| Bifrost projects one facade; Gate, Scribe, Oracle, Forge, `QueryClient`, and transport mechanics are not sibling public clients | PASS | `crates/shared/wyrd-client/src/lib.rs:22`; SDK package roots and bindings delegate to `wyrd_client`; recorded Rust/Python/TypeScript build and integration lanes PASS |
| Server retains durable behavior and public Bifrost table routes use typed wire contracts and structured errors | PASS | `crates/wyrd/wyrd-server/src/bifrost/routes.rs:21-93`; `crates/wyrd-spec/src/vala/api.rs`; recorded OpenAPI/codegen and server-related journeys PASS |
| Stable public error metadata is derive-backed and the TypeScript code union is generated | PASS | `crates/shared/wyrd-error-derive/src/lib.rs`; `crates/wyrd-spec/examples/gen_schemas.rs:273-302`; `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts`; recorded `codegen:check` and TypeScript error test PASS |
| Python expected platform failures use the single structured `WyrdError`, without fake aliases or aggregate `problem` | PASS | deleted `python/py-wyrd/python/wyrd/errors.py`; `sdks/wyrd-sdk-python/python/wyrd/__init__.py:6`; recorded Python unit/typecheck lanes PASS |
| Canonical Python package layout and public projections remain present | FAIL | `architecture/references/languages/python-api-and-stubs.md:38-57` requires `wyrd/errors.py`, but the candidate deletes it and creates no replacement. See `STD-001`. |
| New or materially relocated PyO3 is owned by the Python SDK behind its optional boundary feature | FAIL | `sdks/wyrd-sdk-python/Cargo.toml:18-32` makes PyO3 and every owner-crate `python` feature unconditional. See `STD-002`. |
| Only the Python SDK enables retained owner-crate Python features; production wheel excludes testing | PASS | manifest feature search confines production owner-feature activation to `sdks/wyrd-sdk-python`; recorded `check:pyo3-scope` and `check:py-wheel-no-testing` PASS |
| PyO3 boundary uses `Bound`, detaches blocking work, does not retain borrowed Python values across awaits, and uses the shared runtime | PASS | `sdks/wyrd-sdk-python/src/state/mod.rs`; `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; recorded Python lanes and lints PASS for Rust/Clippy |
| Every new or materially relocated Rust item, including private fields/helpers/tests, has workflow-level rustdoc and required error/panic/cancellation sections | FAIL | Multiple undocumented items remain in the relocated Python SDK and consolidated client. See `STD-003`. |
| Rust signatures import types at module scope and use bare names | FAIL | Fully qualified signatures remain in materially relocated code. See `STD-004`. |
| Rust workflow code is struct-centered and async is limited to real IO/composition | PASS | `Cards`, `ArtifactMaterializer`, `WyrdState`, `HydratedBundleReader`, `PythonStateHydrator`, `PythonCardRegistry`, and napi handles own their state; async methods await filesystem/network/runtime work |
| Public TypeScript bindings validate at the napi edge, call Rust owners, and expose generated declarations | PASS | `sdks/wyrd-sdk-ts/native/src/cards.rs:17-277`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts`; committed `index.d.ts`/`index.d.cts`; recorded `ts:napi:check`, build, typecheck, unit and integration PASS |
| Every first-class user-facing SDK capability has a real SDK → server → SDK journey covering its shipped operations and realistic negative paths | FAIL | The new Rust SDK journey never calls the public Cards `list` or `delete` operations, although the Python and TypeScript journeys exercise listing. See `STD-005`. |
| Live-server/Postgres tests are gated and use repository-managed lifecycle wrappers | PASS | `sdks/wyrd-sdk-rust/tests/cards_state.rs:132-138`; `mise.toml` Card/CLI/WyrdState outer/inner tasks; recorded five Postgres lanes and `test:postgres:inventory` PASS |
| Generated OpenAPI, stubs, declarations, schemas, and runtime MCP catalog have owning checks | PASS | `mise.toml` codegen and napi tasks; recorded `codegen:check`, `ts:napi:check`, MCP exact discovery, and `check:proto-drift` PASS |
| Docs and examples use current Wyrd paths and contain no removed package-root references | FAIL | Five `CodeFromFile` entries still point at deleted `python/py-wyrd` paths. See `STD-006`. |
| Required format and lint gates ran for each changed language | FAIL | Rust `fmt`/`lints` are recorded, but neither `mise run py:format` nor `mise run py:lints` appears in the task evidence despite changed Python files. See `STD-007`. |
| Public docs, examples, release, and CI routes point at the relocated SDK roots | FAIL | Release and CI paths are updated, but the stale docs embeds make the rule fail as a whole. See `STD-006`; recorded release path diff otherwise conforms. |
| No check was weakened or bypassed to hide a live invariant | PASS | Boundary checks were retargeted to the new SDK roots; `scripts/checks/client-tier.sh` still checks the real dependency tree; no new production `#[allow]` lacks its required justification |
| No secrets, tenant selectors, raw SQL pools, new URL-fetch paths, or alternate audit paths entered the changed code | PASS | Complete diff contains no new SQL/storage authority, caller-selected tenant field, URL fetch, credential logging, or audit writer; credentials use `SecretString`/boundary-owned strings |
| Generated/source-tree and build-script rules hold | PASS | build scripts remain generated-output-only; `codegen:check` and `git diff --check` are recorded PASS; no tracked native build artifact is in the candidate |

## Material findings

### STD-001 — VIOLATION: canonical `wyrd.errors` module was removed with its obsolete aliases

- Violated rule: `architecture/references/languages/python-api-and-stubs.md` Package Layout requires `sdks/wyrd-sdk-python/python/wyrd/errors.py` as the structured exception export module; `AGENTS.md` §8 requires public Python package exports to agree with the native boundary.
- Location: deleted `python/py-wyrd/python/wyrd/errors.py`; absent `sdks/wyrd-sdk-python/python/wyrd/errors.py`.
- Evidence: the base module exported `WyrdError` plus three prohibited aliases. The candidate correctly removes the aliases but deletes the canonical module itself; `git ls-tree` shows no replacement under the new package root.
- Consequence: `from wyrd.errors import WyrdError`, the authority-defined structured-error import, now fails even though `WyrdError` remains available at `wyrd.WyrdError`.
- Testable correction: add the canonical `wyrd.errors` projection under the new package root exporting only `WyrdError` (no `Cfg*` aliases or new exception classes), generate its matching public typing surface through the existing stub pipeline, add one public-import assertion, and run `codegen:check`, `py:typecheck`, and the Python unit lane.

### STD-002 — VIOLATION: relocated PyO3 boundary is unconditional

- Violated rule: `AGENTS.md` §7 lines 272–275 and `architecture/references/languages/pyo3-boundaries.md` Placement require new/materially relocated PyO3 in `wyrd-sdk-python` behind its boundary feature with `pyo3` optional; Cargo features must be explicit and only the Python SDK may activate retained owner features.
- Location: `sdks/wyrd-sdk-python/Cargo.toml:18-32`.
- Evidence: the only feature is `testing`; `pyo3` is non-optional and ten owner-crate `python` features are enabled unconditionally in normal dependencies.
- Consequence: the SDK crate has no Python-free/default Rust dependency mode, so merely selecting the crate always activates the foreign runtime boundary and all retained migration features, contrary to the required feature boundary.
- Testable correction: introduce the single SDK Python boundary feature, make `pyo3` and the retained owner Python dependencies optional under it, make maturin/repository Python build lanes enable that feature, and prove default-feature and boundary-feature dependency trees plus `check:pyo3-scope`, Python build, and Python tests.

### STD-003 — VIOLATION: materially relocated Rust items lack required rustdoc

- Violated rule: `AGENTS.md` §16 lines 657–671 and `architecture/agent-rules.md` line 35 make rustdoc on every new or materially modified item, including private fields and helpers, a hard merge blocker; fallible functions require `# Errors`.
- Locations: `sdks/wyrd-sdk-python/src/lib.rs:105`; `sdks/wyrd-sdk-python/src/state/registry.rs:12`; `sdks/wyrd-sdk-python/src/state/mod.rs:1102-1107`, `1171-1197`, `1873-1909`, and `2671-2855`; `crates/shared/wyrd-client/src/cards/handle.rs:142-143`.
- Evidence: examples include undocumented `register_submodule`, the undocumented `PythonCardRegistry.registry` and `Cards.engine` fields, undocumented `PyVersionBump.native`/`as_native`, four undocumented option-map fields, and a long run of undocumented fallible boundary helpers such as `option_values`, `resolve_latest_registry`, `delete_registry`, `selector_for_kind`, and `loader_manifest_error`. These items moved into their new owners in this candidate and are therefore within the rule's materially relocated scope.
- Consequence: the candidate fails an explicit `BLOCK_BEFORE_MERGE` repository standard even though ordinary `cargo doc` does not lint private items.
- Testable correction: document every relocated/new Rust item with its workflow role and invariants, add `# Errors`/`# Panics`/cancellation notes where the rule applies, then run the Rust documentation gate and a source audit covering private items in the touched modules.

### STD-004 — VIOLATION: materially relocated signatures retain fully qualified type paths

- Violated rule: `architecture/agent-rules.md` line 9 requires types to be imported at module scope and bare in fields, parameters, returns, bounds, and `where` clauses.
- Locations: `crates/shared/wyrd-client/src/state.rs:507`, `586`, `614`, `1181`; `sdks/wyrd-sdk-python/src/state/mod.rs:2708`, `2796`; `sdks/wyrd-sdk-python/src/bifrost/mod.rs:576`, `588`, `609`, `1002`; `sdks/wyrd-sdk-ts/native/src/lib.rs:247`, `272`, `283`, `969`.
- Evidence: these signatures use paths such as `wyrd_spec::card::agent::AgentCard`, `wyrd_spec::vala::eval::EvalSpec`, `wyrd_spec::reference::CardRef`, `wyrd_client::bifrost::TableConfig`, and `arrow::record_batch::RecordBatch` directly instead of declaring them in the top-level import block.
- Consequence: the moved owners do not expose their dependency manifest in the mandated top-of-module form and fail an explicit repository style boundary.
- Testable correction: import each signature type at the module top and use its bare name; run `mise run fmt` and `mise run lints`.

### STD-005 — VIOLATION: Rust Cards journey omits shipped list and delete operations

- Violated rule: `AGENTS.md` §11 lines 362–389, `architecture/agent-rules.md` line 18, and `architecture/references/languages/testing-workflows.md` require a real journey for every user-facing capability on every first-class surface.
- Location: `sdks/wyrd-sdk-rust/tests/cards_state.rs:132-214`.
- Evidence: the journey registers, replays, gets, checks a denied registration, hydrates, and loads offline state, but it never calls `Cards::list` or `Cards::delete`. Those are public operations on the newly first-class Rust SDK (`crates/shared/wyrd-client/src/cards/handle.rs:293-333`). The lower-level `wyrd-client/tests/cards_transport.rs` transport test is not a `wyrd_sdk` real-server journey and cannot substitute for one.
- Consequence: Rust package wiring or projection regressions in two shipped Cards operations can pass while Python/TypeScript remain green; the first-class Rust surface is not proven end to end at parity.
- Testable correction: extend the existing Rust SDK journey with one typed list assertion and an exact delete followed by an observable not-found/deleted read, then run its exact repository-managed Postgres command and `test:cards:integration`.

### STD-006 — REGRESSION: docs still embed examples from the deleted Python package root

- Violated rule: `AGENTS.md` opening Wyrd-native rule and §§12/16 require current paths, valid docs, and no legacy package paths; `CodeFromFile` throws when its target does not exist.
- Locations: `docs/src/content/docs/how-to/build-an-agent.svx:18,52`; `docs/src/content/docs/how-to/build-a-workflow.svx:18,74,92`.
- Evidence: every listed target begins `python/py-wyrd/examples/...`; that directory does not exist at the candidate, while the files were moved to `sdks/wyrd-sdk-python/examples/...`. `docs/src/lib/components/CodeFromFile.svelte:48` explicitly throws `file not found` for this state.
- Consequence: building or rendering these how-to pages cannot load their embedded Python examples. This also contradicts the recorded claim that the complete docs gate passed at the immutable candidate.
- Testable correction: retarget all five embeds to `sdks/wyrd-sdk-python/examples/...`, verify no removed SDK roots remain outside historical change artifacts, and rerun `mise run docs:check`.

### STD-007 — VIOLATION: mandatory Python format and lint evidence is absent

- Violated rule: `AGENTS.md` §11 lines 413–419 and `architecture/references/languages/testing-workflows.md` Format and lint require `mise run py:format` and `mise run py:lints` whenever Python changes.
- Location: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md:112-160` and its Implementation Evidence section.
- Evidence: the candidate changes Python tests, examples, and check scripts. The recorded verification includes Python tests and `py:typecheck`, but does not record either required Python format or lint lane.
- Consequence: the task's completion evidence does not establish repository-required Python style/lint compliance.
- Testable correction: run and record `mise run py:format` (or, for immutable verification, the existing non-mutating `mise run py:format:check`) and `mise run py:lints`; if formatting changes are produced, commit them and review the new immutable candidate.

## Verification limits

- Reviewed the complete base-to-candidate name/status, stat, and targeted content diffs, surrounding sources, manifests, generated artifacts, checks, and journey tests.
- Credited the command results recorded in the task evidence where the inspected command and source matched. The stale `CodeFromFile` targets are direct source evidence against the reported `docs:check` result and therefore remain a finding.
- The task intentionally excludes the Bifrost aggregate lane; no standards finding is raised for that explicit task-scoped exclusion.
- No commands that mutate the reviewed source were run. `git diff --check` was independently confirmed clean.

## Overall result

**FAIL**

Material repository-rule violations remain: `STD-001`, `STD-002`, `STD-003`, `STD-004`, `STD-005`, `STD-006`, and `STD-007`.
