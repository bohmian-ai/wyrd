# TASK-002 r2 Repository Standards Review

## Immutable Subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `a000c201ae86f584fd5b80349f375e087902fd78`
- Range: complete cumulative `base..candidate` diff (108 files).
- `HEAD` resolved to the candidate before inspection. Only this assigned review report was written; no reviewed source was changed.

## Authority Coverage

| Changed surface | Applicable authority | Result and evidence |
|---|---|---|
| Repository-wide Rust, task/review packet, manifests, generated artifacts, and checks | `AGENTS.md` §§2-12, 14-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `testing-workflows.md` | **FAIL.** Ownership, async, imports, generated-artifact, and gate-shape rules pass, but the hard every-item rustdoc rule remains unmet in newly added Rust items (`SR2-001`). |
| Protocol, client model, observation identity, and first-class SDK parity | `architecture/wyrd-design.md` (Doctrine, Client model, Observation identity, Bifrost); `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture-constraints.md`; `architecture/patterns.md` | **FAIL.** One shared `wyrd-client::Bifrost`, per-row Card/Run identity, and server-stamped tenant/principal identity pass. TypeScript still silently drops a reachable input shape at its runtime boundary (`SR2-002`). |
| Shared Rust client (`WyrdState`, lifecycle, scoped `Run`/`Observe`, dynamic-table cache, queue configuration) | `AGENTS.md` §§3-6, 9-11, 16; `architecture/agent-rules.md`; `languages/rust-core.md`; `architecture/patterns.md`; `domain/telemetry-observations.md`; `domain/analytical-operations-reliability.md` | **PASS.** `WyrdState` owns one `BifrostLifecycle`; the writer pool closes admission under its producer lock before draining; startup/shutdown fencing is terminal; ambiguous drain retains the same owner; the owner-wide describe miss gate rechecks the cache; configured startup reuses the existing `QueueConfig`. |
| `wyrd-queue` JSON-to-Arrow fixed-size identities | `AGENTS.md` §§4-6, 11, 16; `languages/rust-core.md`; `domain/arrow-analytical-interop.md` | **PASS.** Conversion remains schema-width-driven and checked; malformed/wrong-width values are refused and focused regression evidence is recorded. |
| Vala built-in verification tables, catalog/cache/Forge consumers, Iceberg/DataFusion layouts | `architecture/bifrost-design.md` (Table and row identity, verification tables, resource/failure invariants, public surface); `domain/vala-architecture.md`; `olap-serving.md`; `iceberg.md`; `datafusion.md`; `arrow-analytical-interop.md`; `evaluation.md`; `drift-monitoring.md` | **FAIL.** The canonical registry, typed schemas, daily layouts, sensitivity, and managed correlation comply. Newly added `DomainTable` implementation items remain undocumented under the repository's stricter-than-rustdoc rule (`SR2-001`). |
| `wyrd-spec` identifiers, Eval/Drift contracts, errors, and generated schemas | `AGENTS.md` §§2-4, 9-10, 12, 16; `architecture/wyrd-design.md` §Verifier; `languages/errors.md`; `domain/evaluation.md`; `domain/drift-monitoring.md` | **PASS.** The crate remains IO-, async-, SQL-, and PyO3-free; identities are typed; public errors use the derive-backed catalog; schema fixtures are source-derived according to recorded `codegen:check` evidence. |
| Python SDK/PyO3 boundary, package exports, stubs, runtime tests | `AGENTS.md` §§7-8, 11, 16; `languages/pyo3-boundaries.md`; `python-api-and-stubs.md`; `testing-workflows.md`; `telemetry-observations.md` | **PASS.** Wrappers stay in the Python SDK, use the shared runtime bridge, preserve structured errors, validate the required top-level mapping keys before `json.dumps`, read Python's active OTel context at the boundary, and have public-package unit and real-server journey coverage. |
| TypeScript SDK/N-API boundary, declarations, runtime tests | `AGENTS.md` §§2-3, 9, 11-12; `languages/typescript-guide.md`; `languages/errors.md`; `testing-workflows.md`; `telemetry-observations.md` | **FAIL.** The N-API layer remains thin and active OTel context is read in Node, but `strictJson` ignores symbol-keyed own properties and therefore violates the boundary's no-silent-omission rule (`SR2-002`). |
| Server, test server, auth scope fixture, lazy built-in describe, audit publisher | `AGENTS.md` §§2, 6, 9, 11-12; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `domain/analytical-operations-reliability.md`; `architecture/bifrost-design.md` | **PASS.** `wyrd-server` remains the only serving owner; test-only publication suppression is feature-gated; lazy describe uses the catalog owner; denied describe is audited before admission. |
| Vala SQL audit publication and Forge timing | `architecture/agent-rules.md` SQL/audit rules; `architecture/wyrd-security-posture.md` §§Tenant/data isolation and Audit integrity; `architecture/bifrost-design.md` §§Read audit and Forge; `domain/analytical-operations-reliability.md` | **PASS.** SQL stays behind `TenantConn`/`OperatorPool`; callees do not commit; lock timeout now propagates from the aborted transaction; publication keeps the one watermark/frozen-bound protocol; Forge eligibility uses the database clock. |
| Rust/Python/TypeScript journeys and negative describe proof | `AGENTS.md` §11; `languages/testing-workflows.md`; `architecture/wyrd-design.md` doctrine 20 | **PASS.** All three SDK journeys exercise real server write/shutdown/read and unknown-table refusal; a server-backed Rust journey proves denied describe, canonical audit evidence, no cached destination, and zero producers. |
| Tooling (`mise.toml`, scripts, workspace-hack, lockfile) | `AGENTS.md` §§1, 4, 11-12, 15; `architecture/agent-rules.md`; `languages/testing-workflows.md`; `implementation-execution.md` | **PASS.** The changed capability lane covers all three SDKs and boundary checks; the mock allowlist is limited to sanctioned test-only code. No production gate was disabled. |
| Architecture and active change documentation | `AGENTS.md` §§2, 14-16; `architecture/references/README.md`; `languages/spec-driven-development.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` | **PASS.** Lazy built-in describe and caller-owned `vala.datasets` routing are stated in authority; active review/task records remain change artifacts rather than runtime contracts. |

## Applicable Rule Results

| Rule | Result | Exact evidence |
|---|---|---|
| One shared client composition; server/Vala own durable behavior | PASS | `crates/shared/wyrd-client/src/{state.rs,observe/,bifrost/}` and all language wrappers route through `wyrd_client::Bifrost`; no sibling transport, queue, registry, or warehouse was added. |
| Stateful workflows use cohesive concrete owners | PASS | `WyrdState`, `BifrostLifecycle`, `Run`, `Observe`, `Bifrost`, and `WriterPool` own their state and dependencies. |
| Async only at real IO/composition boundaries | PASS | Describe/connect/drain/record await IO; projection, validation, strict serialization, and schema comparison are synchronous. |
| Shutdown closes admission and successful shutdown is terminal | PASS | `WriterPool::shutdown` sets `closed` while holding the producer-map lock (`bifrost/handle.rs:286-315`); lifecycle closure/fencing is at `observe/lifecycle.rs:136-150,182-203`. |
| Dynamic first use converges on one describe and producer | PASS | `Bifrost::writer_table` performs cache-check, owner-wide async gate, cache recheck, then describe (`bifrost/facade.rs:300-319`); focused concurrent test is present. |
| Tenant, principal, Card, Run, and managed-column identity stay typed and authoritative | PASS | Typed IDs remain in `wyrd-spec`; client rows carry optional Card/Run correlation while server/Scribe stamp tenant, principal, and managed columns. |
| Foreign runtime conversion must not silently alter accepted input | **FAIL** | `strictJson` recursively checks only `Object.entries` (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1130`), which excludes symbol-keyed own properties; `SR2-002`. |
| Public stable errors use the derive-backed catalog and consistent projection | PASS | New Rust errors are catalog variants; Python uses the central mapper; TypeScript's local validation metadata exactly matches `WyrdError::Validation`. |
| Every new/materially modified Rust item has intent-bearing rustdoc | **FAIL** | New trait implementation constants/methods and a local test struct/field have no docs (`SR2-001`). |
| Imports live at module top and signatures use imported bare types | PASS | Prior function-scoped traits/helpers were moved to module or test-module imports; `Debug`, `Formatter`, `fmt::Result`, and `Display` are imported. |
| PyO3 stays in Python SDK; N-API remains a thin runtime boundary | PASS | `check:pyo3-scope` and `check:client-tier` passed during this review; wrappers delegate to the shared Rust owner. |
| User-facing Rust/Python/TypeScript capability has real-server journeys | PASS | `sdks/wyrd-sdk-rust/tests/observe_run.rs`, Python `test_observe_journey.py`, and TypeScript `observe-run.test.ts`; recorded `verify:bifrost` includes them. |
| Generated contracts are source-derived and drift-checked | PASS | Task evidence records `codegen:check` and `ts:napi:check` passing; no source/generated mismatch was found in inspection. |
| SQL transaction ownership and audit single-path invariants | PASS | `freeze_publication_range` propagates `55P03` (`audit_staging.rs:235-269`); the caller owns rollback/commit and the existing publisher remains the sole retained-history path. |
| No gate circumvention or production mock leakage | PASS | No new lint suppression; test controls are feature/test gated; recorded mock, unwrap, lint, and wheel checks pass. |

## Prior Finding Closure

| Stable finding | Standards closure | Evidence |
|---|---|---|
| `FIND-TASK-002-1` configured Rust startup | CLOSED | Exact `start_bifrost_with_config(&WyrdClient, Option<TableConfig>, QueueConfig)` exists and `QueueConfig` is re-exported. |
| `FIND-TASK-002-2` exact fixed schema | CLOSED | Ordered count/name/type/nullability comparison at `observe/mod.rs:305-345`; cases cover reorder, extra, type, nullability. |
| `FIND-TASK-002-3` Python key coercion | CLOSED for the approved correction | Boundary checks reduced mapping/dataclass top-level keys before strict stdlib serialization; focused unit cases cover int/float/bool/None. |
| `FIND-TASK-002-4` TypeScript omission/coercion | **NOT CLOSED** | Ordinary unsupported values, cycles, non-finite and unsafe numbers are refused, but own symbol properties remain silently omitted (`SR2-002`). |
| `FIND-TASK-002-5` foreign active spans | CLOSED | Python and Node read their runtime OTel contexts only when both explicit IDs are absent; explicit identity remains authoritative. |
| `FIND-TASK-002-6` concurrent describe | CLOSED | Owner-wide miss gate plus cache recheck; focused concurrency proof. |
| `FIND-TASK-002-7` terminal shutdown | CLOSED | Not-started/starting shutdown closes; losing start cannot publish; success remains closed. |
| `FIND-TASK-002-8` ambiguous retry | CLOSED | State-level test retains and settles the same batch identity on the same owner, then proves closure. |
| `FIND-TASK-002-9` audit timeout transaction | CLOSED | `55P03` propagates; Postgres test holds the lock, observes explicit error, then retries unchanged state. |
| `FIND-TASK-002-10` mandatory Rust documentation | **NOT CLOSED** | Module/struct docs were added, but the hard rule applies to every added item, including trait implementation items and test-local items (`SR2-001`). |
| `FIND-TASK-002-11` imports/signatures | CLOSED | No disallowed added function-scoped import or fully qualified signature type remains. |
| `FIND-TASK-002-12` real negative describe | CLOSED | All SDK journeys assert real unknown-table refusal; one server-backed path asserts RBAC denial, audit row, empty cache, and no producer. |

## Material Proposals

### `SR2-001` — Mandatory rustdoc coverage remains incomplete

- **Classification:** repository-rule violation.
- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, including private/test items, constants, fields, functions, and methods; missing documentation is `BLOCK_BEFORE_MERGE`.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs:25-49`; `tables/eval/observations.rs:25-46`; `tables/eval/result_items.rs:25-54`; `tables/verification/results.rs:25-59`; representative test-local item `crates/shared/wyrd-client/src/observe/tests.rs:752-755`.
- **Evidence:** The remediation added useful module and table-struct rustdoc, but every newly added `DomainTable` implementation still declares undocumented associated constants and methods. The new local `Features` struct and `score` field likewise have no rustdoc. Clippy does not enforce the repository's stronger private/test-item rule, so a green lint result does not close it.
- **Observable consequence:** The candidate violates an explicit hard completion standard even when compilation and lint lanes pass.
- **Testable correction:** Add concise intent/invariant docs to every undocumented item added in `base..candidate`, including the associated constants/methods and test-local items; do not add suppressions. Re-audit the complete added-item set, then run `mise run fmt` and `mise run lints`.

### `SR2-002` — TypeScript strict JSON still silently drops symbol-keyed data

- **Classification:** boundary correctness and public-surface rule violation.
- **Violated rule:** `architecture/logic/run_api.md:124-130`, `changes/active/verified-change-contract/spec.md` REQ-124, `AGENTS.md` §§2, 8-11, and `architecture/references/languages/typescript-guide.md` require the TypeScript boundary to validate before serialization and refuse unsupported values rather than silently omit or coerce them.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1130`, specifically the `Object.entries(node)` traversal at lines 1126-1128; missing case in `wyrd/tests/unit/observe.test.ts:178-218`.
- **Evidence:** `Object.entries` never returns symbol-keyed own properties. A direct call against the built candidate with `{ [Symbol("secret")]: 1, visible: true }` reached native once as `{"visible":true}` instead of throwing a structured validation error. The existing test covers a symbol *value* under a string key, not a symbol key at the root or nested in a plain object.
- **Observable consequence:** Drift/Eval/generic-record callers can receive success while part of the supplied object was discarded before Rust validation or queue admission, contradicting the advertised strict boundary.
- **Testable correction:** In the existing `strictJson` traversal, reject a plain object that has any own symbol key before iterating its string entries. Add root and nested symbol-key cases to the existing table-driven unit test and assert no native call; retain the current serializer and add no dependency.

## Overall Result

**FAIL** — two bounded repository-standard violations remain: incomplete mandatory Rust item documentation and reachable TypeScript silent omission.

## Verification Notes

- Reviewed the complete cumulative diff and surrounding Rust/Python/TypeScript/server/catalog/SQL consumers.
- Independently ran `mise run ts:typecheck`, `mise run check:client-tier`, `mise run check:pyo3-scope`, and `git diff --check`; all passed.
- Independently reproduced `SR2-002` against the built TypeScript package; native received `{"visible":true}` from an input that also owned a symbol-keyed value.
- The task record reports `verify:bifrost`, `test:shared`, `test:wyrd-sdk`, Python unit/integration/typecheck, TypeScript unit/integration/typecheck/N-API, codegen, boundary, format, and lint lanes passing. Those claims were treated as available evidence, not as proof that the two source-level rules above are satisfied.
- Full aggregate lanes were not rerun in this review.
