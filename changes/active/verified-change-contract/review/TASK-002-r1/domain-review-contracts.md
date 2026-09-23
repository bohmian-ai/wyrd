# TASK-002 round 1 — Domain review: public contracts and SDK projections

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4` to candidate `fbfc2591a985b288935180098f892aecdf3b8b49`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Task authority: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Locked interface/schema authorities: `architecture/logic/run_api.md` and `architecture/logic/table_schema.md` under the active change
- Reviewed boundary: `WyrdState` Bifrost lifecycle; `Run`, `Observe`, Drift, Eval, and generic-record authoring contracts; canonical observation records and stable errors; shared `wyrd-client`/`wyrd-queue` projection; Rust, Python, and TypeScript first-class SDK surfaces, declarations, and real journeys.

The candidate was `fbfc2591a985b288935180098f892aecdf3b8b49` at review start. `.codegraph/` is absent, so discovery used `rg`, immutable diff inspection, and direct source reads as allowed by `AGENTS.md`.

## Authority and source coverage

| Boundary | Governing authority | Source and artifact evidence | Result |
|---|---|---|---|
| One state-owned writer and terminal lifecycle | REQ-123/126/127/133; TASK-002 Scenario 1; `run_api.md`; `AGENTS.md` async/lifecycle rules | `wyrd-client/src/state.rs`; `observe/lifecycle.rs`; lifecycle unit tests; Python and TypeScript state wrappers/stubs | **FAIL — DC-1** |
| Immutable invocation and Card-scoped identity | REQ-123/125; Wyrd design “Observation identity”; telemetry observation reference | `observe/mod.rs::{Run,Observe}`; hydrated-state alias index; Rust/Python/TypeScript unit and journey assertions for shared `run_id`, exact UID-bearing subjects, unknown aliases, and sibling isolation | PASS |
| Drift canonical record and tall-row projection | REQ-124/125/126; `table_schema.md`; Drift reference | `observe/drift.rs`; `wyrd-spec::vala::drift::record`; fixed-table catalog; queue projection tests; Rust/Python/TypeScript wrappers and journeys | Shared Rust projection PASS; foreign input conversion **FAIL — DC-2/DC-3** |
| Eval canonical record, media, trace/span identity, and fixed row | REQ-124/125/129; `run_api.md`; `table_schema.md`; Eval and telemetry references | `observe/eval.rs`; `wyrd-spec::vala::eval::{record,media}`; generated Eval record schema; Python Eval media type/stubs; napi and TypeScript options/declarations; SDK tests | Explicit fields and record shape PASS; active-span projection **FAIL — DC-4** |
| Generic explicit-table record routing | REQ-128/132; Bifrost design; `run_api.md` | `bifrost/{facade,table,handle}.rs`; `observe::record`; `wyrd-queue::BatchBuilder`; fixed-size-binary tests; two-table real SDK journeys | PASS, except TypeScript serialization shares DC-2 |
| Stable errors and generated public projection | REQ-123/124/129; error-language references; `AGENTS.md` generated-artifact rules | derive-backed `WyrdError` additions; Python exception mapping and generated `.pyi`; napi lifecycle result; generated TypeScript error-code union and declarations | PASS for catalog projection; boundary validation gaps are DC-2/DC-3 |
| First-class journey coverage | TASK-002 Scenario 7; `AGENTS.md` testing taxonomy | Rust `observe_run.rs`; Python `test_observe_journey.py`; TypeScript `observe-run.test.ts`; unit boundary suites; TASK-002 recorded verification | Happy-path routing and durability PASS; required negative/runtime cases missing for DC-1 through DC-4 |

## Material findings

### DC-1 — INCORRECT — shutdown can report success without closing the state

- **Severity:** Important.
- **Violated obligation:** REQ-133 requires every successful shutdown to leave the `WyrdState` permanently closed and unable to start or write again. TASK-002 Scenario 1 likewise requires permanent closure after successful shutdown.
- **Exact location:** `crates/shared/wyrd-client/src/observe/lifecycle.rs:114-140`; the contrary behavior is asserted by `crates/shared/wyrd-client/src/observe/tests.rs:438-443` and `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:147-151`.
- **Evidence:** `shutdown()` calls `started()` and converts every error into `Ok(())`. In `NotStarted` it therefore returns success while leaving the phase `NotStarted`, so a later `claim()` can start a writer. In `Starting` it also returns success while leaving the in-flight `StartClaim` untouched; that claim can subsequently publish `Phase::Started`, meaning a completed shutdown is followed by a live writer. Only the already-started path transitions to `Closed`.
- **Observable consequence:** shutdown is not a terminal lifecycle barrier. A caller can successfully shut down and later start or, under a start/shutdown race, begin emitting after shutdown returned. This contradicts the public Rust/Python/TypeScript documentation and can strand work during application teardown.
- **Required testable correction:** make shutdown serialize with the lifecycle phase so a successful return commits `Closed` for `NotStarted`, `Starting`, and `Started`, without losing the same-handle retry semantics after an ambiguous writer drain. Add focused tests proving (1) shutdown-before-start makes restart return `WYRD_SDK_409_BIFROST_CLOSED`, and (2) a start/shutdown race cannot publish a writer after shutdown succeeds. Project the same behavior through Python and TypeScript runtime tests.

### DC-2 — INCORRECT — TypeScript silently rewrites unsupported observation values

- **Severity:** Important.
- **Violated obligation:** REQ-124 requires unsupported Drift values, non-finite/unrepresentable numbers, and silent JavaScript omission or coercion to fail before queue admission. `run_api.md` applies the same boundary rule to TypeScript observation serialization, while REQ-129 requires Eval context to be JSON-serializable.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1068-1079,1105-1111,1123-1132,1144-1146`; missing negative coverage in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:38-117`.
- **Evidence:** all three public emits call raw `JSON.stringify`. JavaScript converts `NaN` and infinities to `null`, drops object properties whose values are `undefined` or functions, and returns `undefined` for an unsupported root. A direct Node probe in this review produced `{"ok":1}` from `{ok: 1, missing: undefined}`, `{"value":null}` from both `NaN` and `Infinity`, `{}` from a function-valued field, and `undefined` from an undefined root. The Rust boundary cannot reject a value that JavaScript already removed, and valid sibling fields let a Drift row proceed after such omission.
- **Observable consequence:** the TypeScript SDK can enqueue a different Drift feature map, Eval context, or generic row than the caller supplied. Missing features can change Drift analysis, and coerced/omitted Eval evidence can change a later verdict without any SDK error.
- **Required testable correction:** use one strict TypeScript JSON conversion helper for `drift`, `eval`, `record`, and media that walks the supplied value before serialization and rejects non-finite/unsafe numbers, `undefined`, functions, symbols, bigint, cycles, and every other value `JSON.stringify` would omit or coerce. Keep the shared Rust validation/projection unchanged. Add TypeScript runtime tests for nested omission, non-finite coercion, an unsupported root, and safe-number boundaries, asserting a structured `WyrdError` before the native method is called.

### DC-3 — INCORRECT — Python mappings coerce non-string feature names

- **Severity:** Important.
- **Violated obligation:** REQ-124 explicitly requires Python mappings and dataclass instances to use strict JSON serialization with string keys and non-finite numbers rejected.
- **Exact location:** `sdks/wyrd-sdk-python/src/observe/mod.rs:24-75`; missing mapping-key coverage in `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:75-115`.
- **Evidence:** `json_text` recognizes any `PyMapping` and passes it directly to `json.dumps(..., allow_nan=False)` without checking its top-level keys. Python's encoder accepts integer, float, boolean, and null mapping keys and coerces them to strings; the review probe produced `{"1": "coerced"}` from `{1: "coerced"}`. Rust therefore receives a string key and cannot tell that the Python caller violated the mapping contract.
- **Observable consequence:** `observe.drift({1: value})` can be accepted as feature name `"1"` instead of failing at the Python boundary. The emitted observation is not the language-native mapping the caller authored, and validation behavior differs across the three first-class SDKs.
- **Required testable correction:** validate that every top-level key of a mapping, and of the mapping returned by `dataclasses.asdict`, is a Python `str` before `json.dumps`; retain `allow_nan=False` and the direct Pydantic `model_dump_json()` path. Add Python-runtime tests for integer/float/boolean/null keys that assert the boundary's stable validation error before writer lookup.

### DC-4 — INCOMPLETE — Python and TypeScript active OpenTelemetry spans are never consulted

- **Severity:** Important.
- **Violated obligation:** REQ-129 requires each SDK to prefer explicit trace/span IDs and otherwise attempt to take valid IDs from the active OpenTelemetry span when its runtime exposes one. TASK-002 Scenario 4 explicitly calls for per-language active-span precedence coverage. Runtime-dependent behavior must be tested in its owning Python or Node runtime under `AGENTS.md` testing rules.
- **Exact location:** `crates/shared/wyrd-client/src/observe/eval.rs:123-142,192-214`; `sdks/wyrd-sdk-python/src/observe/mod.rs:166-190`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1114-1132`; journey omissions at Python `test_observe_journey.py:311-312` and TypeScript `observe-run.test.ts:184-185`.
- **Evidence:** the only fallback calls `tracing::Span::current()` inside Rust. The Python boundary does not read `opentelemetry.trace.get_current_span()`, and the TypeScript boundary does not read the active `@opentelemetry/api` context before calling native code. A Python or Node OpenTelemetry span is runtime-local and does not become a Rust `tracing` current span merely because PyO3 or napi invokes Rust. The Python package exposes OpenTelemetry integration through its `otel` extra, and the TypeScript workspace runs with `@opentelemetry/api`, yet all observation tests omit IDs or pass explicit IDs; none opens an active foreign-runtime span around `observe.eval` and reads back the row.
- **Observable consequence:** Eval observations emitted inside normal Python or TypeScript traced operations store null `trace_id`/`span_id` unless callers manually copy IDs. Trace-dependent Eval tasks then lack the promised correlation even though a valid active span was available.
- **Required testable correction:** at each foreign-runtime boundary, when explicit IDs are absent, read the runtime's active OpenTelemetry span context, validate it, and pass the resulting IDs through `EvalObservationOptions`; preserve explicit-ID precedence and the shared Rust fallback for Rust callers. Add Python and Node runtime tests with a valid active span plus explicit-override cases, and assert the persisted fixed-size trace/span bytes in the real SDK journeys.

## Verification limits

- Static review covered the complete base-to-candidate diff for the shared client, canonical records, queue conversion, Python/PyO3 surface and stubs, TypeScript/napi surface and declarations, Rust SDK projection, and the three real journeys. The five physical table definitions and generated Eval schema were inspected for contract alignment; detailed storage/durability behavior remains outside this domain slice.
- TASK-002 records green `test:shared`, `test:wyrd-sdk`, all `verify:bifrost` lanes, Python unit/integration/typecheck, TypeScript unit/integration/typecheck/napi, `codegen:check`, boundary checks, format, lints, and `git diff --check` at the candidate. Those lanes establish the happy paths but contain none of the four required cases above.
- This review ran `git diff --check`, a direct Node serialization probe, and a Python stdlib serialization probe through the repository toolchain. One attempted exact Cargo filter selected zero tests and is not treated as evidence. No full Cargo, Python, TypeScript, Postgres, or generated-artifact lane was rerun.
- Generated Python and TypeScript declarations expose the intended public names and stable new errors. No hand-edited artifact drift was found. The remaining defects are runtime behavior and lifecycle semantics, not declaration-only mismatches.

## Overall result

**FAIL.** The shared Rust identity, fixed-row projection, explicit-table routing, canonical record cleanup, error catalog, and happy-path SDK journeys align with revision 32. The change is not contract-complete because shutdown can succeed without terminal closure, TypeScript can silently mutate observation input, Python can coerce mapping keys, and the two foreign runtimes do not project active OpenTelemetry span identity.
