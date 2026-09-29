# TASK-002 round 2 — Domain review: public contracts and SDK projections

## Review Findings

### Critical

None.

### Important

- **`DC-R2-001` — INCORRECT — TypeScript still silently omits symbol-keyed observation data.**
  - **Violated obligation:** REQ-124 requires unsupported values and silent JavaScript omission or coercion to fail before queue admission. `run_api.md` likewise requires the TypeScript boundary to reject values that `JSON.stringify` would silently omit. This is also incomplete closure of prior `FIND-TASK-002-4`.
  - **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1094-1132`; missing case in `sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:177-220`.
  - **Evidence:** `strictJson` checks only `Object.entries(node)`. Enumerable symbol-keyed properties are absent from `Object.entries` and from `JSON.stringify`, so they pass validation and disappear. Against the candidate build, `new Observe(fakeNative).eval({ visible: 1, [Symbol("secret")]: 2 })` called native with `{"visible":1}` instead of raising the structured validation error. The existing test covers a symbol *value* under a string key, not a symbol key.
  - **Consequence:** Eval context and generic records can be enqueued with caller-supplied data silently removed. That is an observable first-class SDK contract violation and can change later evaluation evidence.
  - **Required outcome:** reject own symbol keys before iterating string-keyed entries. Keep the single boundary serializer and existing Rust validation; no new serializer or dependency is needed.
  - **Closure proof:** add a TypeScript runtime test that supplies an enumerable symbol-keyed property to Drift, Eval, and generic record inputs, asserts `WYRD_SPEC_400_VALIDATION`, and proves the native method was not called.

- **`DC-R2-002` — MISSING — Rust and TypeScript Eval journeys do not prove trace identity through the real SDK → N-API/Rust → queue → Arrow path.**
  - **Violated obligation:** AC-026 requires the Rust, Python, and TypeScript Eval journeys to preserve valid explicit or active-span IDs, round-trip non-null IDs through `FixedSizeBinary(16)/(8)`, and reject invalid span/trace and malformed-media inputs before admission. The testing taxonomy does not allow a wrapper unit test to replace that required journey. This leaves prior `FIND-TASK-002-5` only partially closed.
  - **Exact location:** Rust emits only `EvalObservationOptions::default()` and queries no trace columns at `sdks/wyrd-sdk-rust/tests/observe_run.rs:257-271,379-385`; TypeScript emits only `{ answer: "yes" }` and queries only `context`, `card_uid`, and `run_id` at `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:184-186,239-250`. Python supplies the missing real-boundary proof at `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:269-282,333-340`.
  - **Evidence:** the new TypeScript unit test at `observe.test.ts:229-260` proves only that the ergonomic wrapper forwards active IDs to a fake native object. It cannot prove N-API parsing, typed ID construction, queue conversion, fixed-size Arrow bytes, or persisted readback. The Rust projection unit test similarly bypasses the real SDK/server path. Neither Rust nor TypeScript real journey contains the required invalid pair or malformed-media pre-admission assertion.
  - **Consequence:** candidate verification does not establish the locked trace/media contract for two of three first-class SDKs; a binding or queue regression can pass all recorded lanes while dropping or corrupting correlation.
  - **Required outcome:** extend the existing Rust and TypeScript observation journeys rather than add another harness. Emit a valid trace/span pair (and the required optional session/media shape), read back `trace_id` and `span_id`, compare exact bytes/hex, and assert malformed media plus span-without-trace fails before enqueue. For TypeScript, include an active-span emission so the remediation path crosses the real N-API boundary.
  - **Closure proof:** the focused Rust and TypeScript real-server journeys pass with exact non-null binary readback and the negative pre-admission assertions, then the recorded `test:bifrost:journey:{sdk,typescript}` / `verify:bifrost` lanes pass.

- **`DC-R2-003` — MISSING — AC-025's required per-SDK dynamic-table and startup journey proofs remain below the user-journey tier.**
  - **Violated obligation:** AC-025 requires the Rust, Python, and TypeScript SDK journeys to fail startup when either fixed table cannot be described and to show cached dynamic-table reuse, concurrent first-use convergence, and the stale-schema fingerprint refusal. AGENTS.md and `testing-workflows.md` state that supporting unit/integration coverage cannot substitute for a required user journey.
  - **Exact location:** the three journeys perform one successful startup and one write to each dynamic table at `sdks/wyrd-sdk-rust/tests/observe_run.rs:344-349,386-390`, `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:322-341`, and `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:167-187`. The incompatible-schema and concurrency proofs exist only in shared Rust tests at `crates/shared/wyrd-client/src/observe/tests.rs:361-439,876-925`.
  - **Evidence:** none of the three SDK journeys attempts a fixed-table startup refusal, repeats a dynamic-table write to demonstrate cache reuse, races the first use, or exercises a stale writer after a server schema change. The remediation correctly added real unknown-table refusal to all three journeys and an audited denied describe to `pg_bifrost_e2e`, but those close only the prior `FIND-TASK-002-12` boundary.
  - **Consequence:** language-boundary construction and lifecycle regressions can remain invisible even though the shared owner tests are green, contrary to the approved acceptance criterion assigned to TASK-002.
  - **Required outcome:** add the missing cases to the existing three journeys, reusing their server and registered tables. Do not create another queue, cache, or harness.
  - **Closure proof:** each SDK journey exercises fixed-table startup refusal and cached reuse; the required real-server evidence also covers convergence and stale-schema refusal through the public SDK surfaces, followed by the three capability journey lanes.

### Suggestions

None.

## Open Questions

None. The approved revision 32 contracts decide the required behavior and test tier.

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Immutable subject: base `c8bb490ad814c0c7770cac33ed7779897ff776e4` to candidate `a000c201ae86f584fd5b80349f375e087902fd78`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Task authority: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Locked interface/schema authorities: `architecture/logic/run_api.md` and `architecture/logic/table_schema.md` under the active change
- Reviewed boundary: `WyrdState` writer lifecycle; immutable `Run`/`Observe` scope; Drift, Eval, and generic-record validation and projection; fixed schema preflight; shared Bifrost explicit-table routing; Rust/Python/TypeScript projections, declarations, and TASK-002 journeys.

The candidate was `a000c201ae86f584fd5b80349f375e087902fd78` at review start and remained that commit after inspection. `.codegraph/` is absent, so source discovery used `rg`, immutable diff inspection, and direct source reads as permitted by `AGENTS.md`.

## Authority and source coverage

| Boundary | Governing authority | Source and artifact evidence | Result |
|---|---|---|---|
| State-owned writer, configured startup, and terminal lifecycle | REQ-123/126/127/133; `run_api.md`; Bifrost design | `state.rs`; `observe/lifecycle.rs`; lifecycle and ambiguity tests; Rust SDK configured journey | PASS |
| Immutable invocation and Card-scoped correlation | REQ-123/125; observation identity doctrine | `observe/mod.rs`; hydrated alias index; three SDK wrappers and journey readback | PASS |
| Drift canonical record and tall projection | REQ-124/125; `table_schema.md` | `observe/drift.rs`; `wyrd-spec::vala::drift::record`; projection/input tests; SDK wrappers | PASS except TypeScript strict serialization under `DC-R2-001` |
| Eval context/media/trace projection | REQ-124/125/129; AC-026 | `observe/eval.rs`; canonical Eval record/media contracts; Python/PyO3 and TypeScript/N-API wrappers; SDK tests | FAIL — `DC-R2-002` |
| Fixed-table exact preflight and dynamic explicit-table routing | REQ-127/128; AC-025 | `require_projection`; `Bifrost::writer_table`; describe gate; shared tests; three SDK journeys | Runtime owners PASS; required journey tier FAIL — `DC-R2-003` |
| Stable errors and generated declarations | error/PyO3/Python/TypeScript references | derive-backed catalog; Python stubs; N-API and TypeScript declarations; codegen evidence | PASS |
| Exact five-table public schema projection | REQ-121/122; `table_schema.md`; Bifrost design | canonical record types; fixed projection declarations; Vala table definitions/tests; recorded codegen/catalog lanes | PASS within this slice |

Applicable authority reviewed includes `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, the spec-driven-development workflow, Rust/error/PyO3/Python/TypeScript/testing references, and telemetry-observation identity guidance.

## Prior finding closure

| Prior finding | Round-2 status | Validated closure or remaining issue |
|---|---|---|
| `FIND-TASK-002-1` configured Rust startup | CLOSED | `start_bifrost_with_config` delegates to existing configured construction; `QueueConfig` is re-exported through `wyrd-client` and `wyrd-sdk`; the Rust journey compiles the locked call. |
| `FIND-TASK-002-2` exact fixed schema | CLOSED | Full ordered name/type/nullability equality is enforced and tested for reordered, extra, retyped, and renullabled fields. |
| `FIND-TASK-002-3` Python string keys | CLOSED | Mapping/dataclass reduction now checks keys before `json.dumps`; focused Python tests cover the coercible native key types. |
| `FIND-TASK-002-4` TypeScript strict JSON | **OPEN** | Common validation was added, but symbol-keyed fields still disappear; see `DC-R2-001`. |
| `FIND-TASK-002-5` foreign active spans | **PARTIAL** | Python crosses the real journey and TypeScript forwards to a fake native object, but Rust/TypeScript journey obligations remain open; see `DC-R2-002`. |
| `FIND-TASK-002-6` concurrent first describe | CLOSED at owner behavior | Owner-local miss gate, cache recheck, and focused concurrency test prove one describe/producer. AC-025's required SDK-journey projection remains `DC-R2-003`. |
| `FIND-TASK-002-7` terminal shutdown | CLOSED | Never-started and controlled start/shutdown races end closed; a late `StartClaim` cannot publish. |
| `FIND-TASK-002-8` ambiguous retry | CLOSED | State-level test retains and settles the same producer/batch across retry, then refuses writes/restart. |
| `FIND-TASK-002-9` audit timeout | Outside this domain slice | The remediation diff and recorded evidence were noted; SQL transaction/audit correctness belongs to the data/security reviewers. |
| `FIND-TASK-002-10` module rustdoc | CLOSED | Added table modules/items carry intent-bearing rustdoc without suppression. |
| `FIND-TASK-002-11` import/signature placement | CLOSED | Reviewed production additions use module-scope imports and bare imported signature types. |
| `FIND-TASK-002-12` real describe refusal | CLOSED | All three SDK journeys hit a real unknown-table describe; the server-backed denied path asserts the stable authorization error, canonical audit row, empty cache, and zero producers. |

## Verification Notes

- Reviewed the cumulative base-to-candidate diff plus the remediation-only range `fbfc2591a985b288935180098f892aecdf3b8b49..a000c201ae86f584fd5b80349f375e087902fd78`, relevant full source bodies, generated declarations, and the recorded TASK-002 verification evidence.
- TASK-002 records green `verify:bifrost`, `test:shared`, `test:wyrd-sdk`, Python unit/integration/typecheck, TypeScript unit/integration/typecheck/N-API, codegen, client/PyO3 boundary, format, lint, and diff-check lanes at `a96820fa`; `a000c201` changes only the task record. Those runs are credible for the cases present but cannot prove omitted acceptance cases.
- This review did not rerun the broad Cargo/Postgres/Python/TypeScript lanes. It ran a direct Node probe against the candidate build that reproduced `DC-R2-001`. No recorded command covers the missing Rust/TypeScript AC-026 journey readback or the per-SDK AC-025 cases in `DC-R2-002`/`DC-R2-003`.
- Detailed server durability, SQL audit publication, and cross-tenant admission are intentionally left to their assigned domain reviews.

## Overall result

**FAIL.** The remediation materially improves the public SDK contract and closes most prior findings, but REQ-124 remains behaviorally violated for symbol-keyed TypeScript input, and the mandatory AC-026/AC-025 user-journey proofs are still incomplete. All three corrections are bounded implementation/test work within approved revision 32; no specification revision is required.
