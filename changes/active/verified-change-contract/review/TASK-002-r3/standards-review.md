# TASK-002 R3 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Range reviewed: the complete cumulative `c8bb490a..04f73570` diff
- Candidate rechecked before completion: `04f73570397d5123eb767abafa60d016c37de1db`
- CodeGraph: not used because the repository has no `.codegraph/` directory

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active task, remediation evidence, logic notes, and Bifrost architecture prose | `AGENTS.md` §§1, 12, 14-16; `architecture/agent-rules.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | FAIL — one recorded verification claim contradicts the asserted error code in source (`STD-R3-3`) |
| Canonical Drift/Eval records, identifiers, media, errors, and generated JSON schemas | `architecture/wyrd-design.md` (client model, observation identity, Verifier/Eval/Drift); `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `architecture/references/domain/{telemetry-observations,evaluation,drift-monitoring}.md` | PASS |
| Shared Rust client, state-owned lifecycle, scoped Run/Observe projection, queue conversion | `AGENTS.md` §§3-6, 9-10, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md`; `architecture/references/domain/arrow-analytical-interop.md` | PASS |
| Bifrost catalog, namespaces, fixed tables, schema fingerprints, storage cache, Forge and Oracle test adjustments | `architecture/bifrost-design.md`; `architecture/references/domain/{vala-architecture,olap-serving,arrow-analytical-interop,analytical-operations-reliability}.md`; `AGENTS.md` §§3-6, 10-12, 16 | PASS except for Rust type/import form in modified harness/test code (`STD-R3-1`) |
| Vala SQL audit publication and Forge task clock-domain changes | `architecture/agent-rules.md` SQL/audit rules; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/references/languages/rust-core.md`; `architecture/references/domain/analytical-operations-reliability.md` | PASS |
| Server lifecycle and `test-support`-gated audit-publication control | `AGENTS.md` §§2-3, 9, 11; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; `architecture/references/languages/testing-workflows.md` | PASS |
| `wyrd-testing` credentials, describe probes/faults, port reservation, and language test projections | `AGENTS.md` §§3-7, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/{rust-core,testing-workflows}.md` | FAIL — new/modified dependencies are not consistently declared at module top or used by bare name (`STD-R3-1`) |
| Python PyO3 wrappers, public package exports, stubs, unit and integration journeys | `AGENTS.md` §§7-8, 11, 16; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows,errors}.md` | PASS |
| TypeScript wrappers, N-API bindings, generated declarations, unit and integration journeys | `AGENTS.md` §§2-3, 11; `architecture/references/languages/{typescript-guide,testing-workflows,errors}.md` | FAIL — two new public object shapes use open interfaces without a declaration-merging need (`STD-R3-2`) |
| Cargo manifests/lockfile/workspace-hack, mise tasks, journey script, mocks allowlist | `AGENTS.md` §§1, 4, 11-12, 15; `architecture/agent-rules.md`; `architecture/references/languages/{implementation-execution,testing-workflows}.md` | PASS |

## Applicable-rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable behavior remains in Rust/server owners; SDKs only project shared `wyrd-client` behavior | Observation construction/routing is owned by `crates/shared/wyrd-client/src/observe`; Python and N-API wrappers convert boundary values and delegate; fixed schemas remain in Vala | PASS |
| One state-owned Bifrost facade; no second transport, queue, registry, or client-side schema authority | `WyrdState` owns `BifrostLifecycle`; `StartedBifrost` holds the shared facade and described fixed destinations; SDK wrappers call it | PASS |
| Observation identity preserves publisher/subject split and typed identifiers | Drift/Eval payload records omit Run and Verifier identity; Bifrost correlation carries `CardRef`/`RunId`; server continues to stamp tenant, publisher, and Card UID | PASS |
| Public errors use the derive-backed catalog and retain cross-language metadata | Added lifecycle/observation variants are declared on `WyrdError`; Python and TypeScript project structured catalog metadata | PASS |
| Fixed analytical schemas and binary identities remain explicit and fail closed | Vala owns the five table definitions; client fixed projections compare ordered name/type/nullability; queue decoding rejects malformed/wrong-width hex | PASS |
| Rust code is struct-centered and async is limited to IO/composition | Stateful lifecycle lives on `BifrostLifecycle`, scoped operations on `Run`/`Observe`, and table-description/network/drain calls own the async boundaries | PASS |
| Every added/materially changed Rust item has maintainer-grade rustdoc, with error/panic/cancellation text where applicable | Complete declaration audit across the cumulative Rust diff, including remediation-added harness and journey items | PASS |
| Imports form the module dependency manifest and signature types use imported bare names | New `service_account_by_card_ref` is added to a `use` block after module items; new/modified port helpers and Oracle cutoff constants spell standard-library types with qualified paths despite existing/top-level import requirements | FAIL (`STD-R3-1`) |
| PyO3 remains a thin owned boundary with correct runtime placement | New PyO3 resides in `sdks/wyrd-sdk-python`, uses `Bound`, converts before delegation, detaches blocking record IO, and does not leak Python types across await/core boundaries | PASS |
| Python public API, package exports, generated stubs, and public-import tests remain aligned | Native registration, `wyrd.eval`/`wyrd.observe` projections, `.pyi` outputs, unit coverage, and a real Python journey are present; recorded `codegen:check` and `py:typecheck` pass | PASS |
| New TypeScript object shapes use `type` unless declaration merging is required | `EvalMediaRef` and `EvalOptions` are new exported `interface` declarations with no declaration-merging consumer or rationale | FAIL (`STD-R3-2`) |
| TypeScript/N-API behavior delegates durable semantics and preserves stable errors | Hand-authored TS validation is boundary-only; N-API calls shared `Run`/`Observe`; generated declarations and real Node journey are present | PASS |
| User-facing behavior has real Rust, Python, and TypeScript client-to-server-to-client journeys | All three journeys register the Service graph, start the state-owned writer, switch views, emit/read rows, and exercise negative/edge paths | PASS |
| Integration tests earn external placement and runtime-specific tests stay in their runtime | Rust external tests use real Postgres/server boundaries; Python and Node lifetime behavior stays in their respective integration suites | PASS |
| Test-only hooks cannot alter production behavior by merely compiling ordinary production features | `AppState::audit_publication_disabled` and its setter are `cfg(feature = "test-support")`; the non-test-support server branch always creates the publisher normally | PASS |
| SQL tenancy, transaction ownership, and audit path remain intact | Tenant operations use `TenantConn`; operator Forge paths retain `OperatorPool`; audit publication still reads the canonical staging path, and the timeout propagates rather than fabricating idle success | PASS |
| No gate is weakened to make the change pass | New ignored Rust cases are gated journeys run with `--run-ignored=all`; the mocks allowlist entries are test-only and documented; Clippy allows carry sanctioned N-API justification and `mise run check:clippy-allow-audit` passes | PASS |
| Generated artifacts are regenerated and verified, not treated as source | JSON schemas, Python stubs, and N-API declarations have matching source changes; recorded `codegen:check` and `ts:napi:check` pass | PASS |
| Completion evidence accurately states what source/tests prove | The R2 FIND-4 row says the TS unit table asserts `WYRD_SDK_400_INVALID_OBSERVATION`, while both the implementation and test assert `WYRD_SPEC_400_VALIDATION` | FAIL (`STD-R3-3`) |
| Required formatting and changed-surface verification are credible | Recorded capability, owner, SDK, type, codegen, lint, and format lanes passed; after the final Oracle test-only commits, focused tests and Clippy are recorded. Reviewer additionally ran `mise exec -- cargo fmt --all -- --check`, `mise run check:clippy-allow-audit`, and cumulative `git diff --check`; all passed | PASS |

## Material findings

### STD-R3-1 — Modified Rust code does not keep imports at module top or use bare signature types

- Classification: VIOLATION
- Violated rule: `architecture/agent-rules.md` requires every `use` at the top of its module and imported bare types in signatures/fields/bounds; `AGENTS.md` §16 applies repository style to every new or materially modified Rust item.
- Locations:
  - `crates/wyrd/wyrd-testing/src/server.rs:144-160` — the candidate adds `service_account_by_card_ref` to a `use` block that occurs after `TestOraclePeerCredentials` and its impls.
  - `crates/wyrd/wyrd-testing/src/server.rs:4455-4457,4481,4486,4509` — new and materially changed helpers use `std::ops::Range`, `std::sync::OnceLock`, `std::sync::atomic::AtomicU32`, and `std::net::SocketAddr` instead of top-level imports and bare signature types.
  - `crates/wyrd/wyrd-server/tests/pg_router_smoke.rs:1106,1115` — the final remediation commits add cutoff constants typed as `std::time::Duration` rather than importing `Duration` with the module dependencies.
- Evidence: these exact lines are additions or material modifications in `c8bb490a..04f73570`; `server.rs` already imports `SocketAddr` at module top, demonstrating that the qualified return types are unnecessary.
- Observable consequence: the changed modules conceal part of their dependency surface in item bodies/signatures and leave newly touched Rust in the explicitly prohibited structural form, even though compilation and formatting succeed.
- Testable correction: hoist the newly used standard-library and SQL symbols into the existing top-of-module import groups, use bare `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, and `Duration` in the changed items, and remove `service_account_by_card_ref` from the late block; prove with `mise run fmt`, `mise run lints`, and `mise run check:clippy-allow-audit`.

### STD-R3-2 — New public TypeScript options are open interfaces without a merging requirement

- Classification: VIOLATION
- Violated rule: `architecture/references/languages/typescript-guide.md` requires types over interfaces unless declaration merging is required.
- Location: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1044-1066` (`EvalMediaRef`, `EvalOptions`).
- Evidence: both declarations are newly authored public data shapes, have no merge declarations or extension consumer, and are used only as closed observation input types.
- Observable consequence: consumers may declaration-merge fields into what is meant to be the exact Eval authoring contract, weakening the closed API shape and diverging from the repository's TypeScript convention.
- Testable correction: express both shapes as exported readonly object type aliases without changing fields or runtime behavior; prove with `mise run ts:typecheck`, `mise run ts:test:unit`, and `mise run ts:test:integration`.

### STD-R3-3 — The committed remediation evidence names the wrong TypeScript error code

- Classification: VIOLATION
- Violated rule: `AGENTS.md` §§12 and 14 plus `architecture/references/languages/spec-driven-development.md` require accurate, credible acceptance and verification evidence.
- Location: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md:397`.
- Evidence: the row claims the symbol-key cases assert `WYRD_SDK_400_INVALID_OBSERVATION`; `invalidObservationInput` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1068-1077` constructs `WYRD_SPEC_400_VALIDATION`, and `tests/unit/observe.test.ts:170` asserts that latter code.
- Observable consequence: a maintainer or later immutable review following the task record expects a stable failure contract that the shipped SDK and its passing test do not provide.
- Testable correction: change only the evidence row to name `WYRD_SPEC_400_VALIDATION` (the remediation task required the existing TypeScript validation path, not a new code), then rerun the recorded focused unit command to keep the evidence exact.

## Verification limits

- This was a read-only standards audit, not a replay of the full capability suite.
- The full `verify:bifrost` result is recorded at `4fc251ce`; the later Oracle test-only commits record the complete server integration lane for the first fix and focused tests plus Clippy for the follow-up. No production Oracle behavior changed in those commits.
- Reviewer-local non-mutating checks passed: `mise exec -- cargo fmt --all -- --check`, `mise run check:clippy-allow-audit`, and `git diff --check c8bb490a..04f73570`.

## Overall result

**FAIL** — three bounded repository-standards violations remain (`STD-R3-1`, `STD-R3-2`, `STD-R3-3`).
