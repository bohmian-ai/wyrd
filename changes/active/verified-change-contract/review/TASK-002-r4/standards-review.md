# TASK-002 R4 repository-standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Range reviewed: `c8bb490a..b56560e5` (complete cumulative TASK-002 implementation and remediation)
- Candidate rechecked before writing: `git rev-parse HEAD` returned the candidate exactly.
- The current user explicitly allows AI co-author trailers for this review; they are therefore excluded from findings.

## Authority coverage

| Changed surface | Applicable authority read | Coverage |
|---|---|---|
| Shared Rust client (`WyrdState`, `Bifrost`, scoped observations, lifecycle/cache) | `AGENTS.md` §§2-6, 9-12, 16; `architecture/agent-rules.md`; `wyrd-design.md`; `wyrd-doctrine.mdx`; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `run_api.md` | Complete: owner placement, struct-centered shape, bounded async, stable errors, docs, and tests inspected. |
| Queue JSON-to-Arrow conversion | `AGENTS.md` §§3-6, 10-12, 16; `agent-rules.md`; `bifrost-design.md`; `references/domain/arrow-analytical-interop.md`; `references/domain/olap-serving.md`; `table_schema.md` | Complete: generic fixed-size-binary conversion and schema-driven test inspected. |
| Vala built-in table catalog and fixed schemas | `AGENTS.md` §§2-6, 9-12, 16; `agent-rules.md`; `bifrost-design.md`; `references/domain/vala-architecture.md`; `references/domain/olap-serving.md`; `table_schema.md` | Complete: ownership, lazy materialization, field order/type/nullability, partition/Bloom declarations, and schema tests inspected. |
| Eval/Drift wire contracts | `AGENTS.md` §§2-4, 9-10, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `references/domain/evaluation.md`; `references/domain/drift-monitoring.md`; `references/domain/telemetry-observations.md`; `run_api.md` | Complete: canonical record changes, typed identifiers, media, correlation split, and generated schema evidence inspected. |
| Python SDK and PyO3 projection | `AGENTS.md` §§3, 7-12, 16; `references/languages/pyo3-boundaries.md`; `references/languages/python-api-and-stubs.md`; `references/languages/errors.md`; `references/languages/testing-workflows.md` | Complete: boundary placement, runtime bridge, public exports/stubs, structured errors, and unit/journey coverage inspected. |
| TypeScript SDK and N-API projection | `AGENTS.md` §§2-3, 9-12; `references/languages/typescript-guide.md`; `references/languages/errors.md`; `references/languages/testing-workflows.md`; `run_api.md` | Complete: thin native projection, strict JSON boundary, closed readonly types, error projection, and Node-owned tests inspected. |
| Server/test harness, auth/audit test seams | `AGENTS.md` §§2-6, 9, 11-12, 16; `agent-rules.md`; `wyrd-security-posture.md`; `references/architecture/patterns.md`; `references/languages/testing-workflows.md` | Complete: `test-support` gating, production audit-publisher split, credentialed Card principal, and real-server fixtures inspected. |
| Vala SQL audit/Forge bounded corrections | `AGENTS.md` §§2-6, 9, 11-12, 16; `agent-rules.md`; `bifrost-design.md`; `references/domain/analytical-operations-reliability.md` | Complete: `TenantConn`/`OperatorPool` boundaries, transaction ownership, bounded audit lock wait, and database-clock eligibility inspected. |
| Tooling, manifests, generated artifacts, docs/evidence | `AGENTS.md` §§1, 4, 8, 11-14, 16; `agent-rules.md`; `references/languages/spec-driven-development.md`; `references/languages/implementation-execution.md`; `references/languages/testing-workflows.md` | Complete: dependency placement, capability lanes, generated-file policy, check changes, and recorded commands/results inspected. |

## Applicable-rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable behavior remains in Rust owners; SDKs are projections over `wyrd-client`. | Shared behavior is owned by `crates/shared/wyrd-client/src/observe/`, `state.rs`, and `bifrost/facade.rs`; Python wraps it in `sdks/wyrd-sdk-python/src/observe/mod.rs`, and TypeScript's native layer delegates to it. | PASS |
| One cohesive concrete owner; no duplicate queue, producer pool, transport, or schema authority. | `WyrdState` owns `BifrostLifecycle`; `StartedBifrost` owns the one facade and fixed destinations; `Bifrost::insert_into` continues through the existing `WriterPool`; the cumulative diff adds no second transport or pool. | PASS |
| Async is limited to real IO/composition. | Drift/Eval projection and validation remain synchronous; `record` is async only for first-use describe; start/shutdown await transport and draining. | PASS |
| Public failures use stable structured Wyrd errors. | Rust uses catalog-backed `WyrdError`; Python preserves typed Wyrd exceptions; TypeScript `invalidObservationInput` uses the existing `WYRD_SPEC_400_VALIDATION` contract and the committed R2/R3 evidence now names that code correctly. | PASS |
| Observation identity preserves authenticated writer versus observed subject. | `Run::correlation` supplies `CardRef` plus one `RunId`; the queue adds managed correlation while Gate/Scribe stamp tenant/principal/Card UID; canonical Drift/Eval records no longer author run or Verifier identity. | PASS |
| Fixed schemas and generic Arrow mapping stay in their owners and map by field identity. | The five Vala `DomainTable` declarations define exact authored schemas; `require_projection` compares the complete ordered schema; `wyrd-queue/src/batch_builder.rs` handles `FixedSizeBinary` generically from canonical hex. | PASS |
| Bifrost remains bounded and schema-fenced. | The existing bounded queue and `WriterPool` are reused; first-use table describe is gated and cached; startup checks both fixed schemas; stale fingerprints are refused by the server journey proof. | PASS |
| Audit and tenancy rules remain intact. | Lazy built-in describe follows the existing catalog owner; describe authorization uses the canonical audited server path; the added audit lock timeout propagates `55P03` rather than treating an aborted transaction as idle; SQL signatures retain sanctioned `TenantConn`/`OperatorPool` boundaries. | PASS |
| Test-only server controls cannot change production behavior. | `AppState::audit_publication_disabled` and its startup branch are both `cfg(feature = "test-support")`; the non-test branch still constructs `AuditPublisher` unconditionally from state. | PASS |
| Rust imports and signatures expose dependencies at module top with bare declaration types for changed items. | R3 moves `Range`, `OnceLock`, `AtomicU32`, `SocketAddr`, `Duration`, and `service_account_by_card_ref` to module-top imports and uses bare names in the cited declarations; no new function-local import or qualified declaration type remains in the R3 correction. | PASS |
| New/materially modified Rust items carry intent-focused rustdoc, including errors, panics, cancellation, and fields where applicable. | The cumulative declaration audit covers the observation owners, lifecycle, queue conversion, Vala table types, SQL changes, test harness hooks, and tests; the R2 rustdoc remediation documents the previously missing associated and test-local items without suppression. | PASS |
| PyO3 remains at the Python SDK boundary and uses the shared runtime. | New observation wrappers live in `sdks/wyrd-sdk-python/src`; inputs convert before calling Rust owners; no `Bound` crosses an await and no ad-hoc runtime was added; `check:pyo3-scope` is recorded passing. | PASS |
| Python exports, stubs, and runtime tests form one public surface. | `wyrd.eval`, `wyrd.observe`, and `wyrd.state` projections and generated stubs are present; tests import public `wyrd` modules; `codegen:check`, Python unit/integration/typecheck/format/lint lanes are recorded passing. | PASS |
| TypeScript uses closed immutable shapes and refuses lossy observation serialization before N-API. | `EvalMediaRef` and `EvalOptions` are readonly type aliases; `strictJson` remains the single serializer and checks plain-object own keys plus array own keys before traversing; table-driven tests exercise root/nested refused values across Drift, Eval, and generic record with zero native calls. | PASS |
| Runtime-owned tests execute in their owning language and every shipped SDK has a real journey. | Rust uses `observe_run.rs`; Python uses `test_observe_journey.py`; TypeScript uses `observe-run.test.ts`; Node-only active-span behavior is exercised through Node/N-API, not emulated in Rust. | PASS |
| User journeys include real client-server-client happy, edge, and negative paths. | All three journeys start the state-owned writer, switch Card views, write/read Drift/Eval/two datasets, verify session/media/trace identity, reject malformed inputs/reserved/unknown tables, prove startup refusal/cached describes, and drain at shutdown; server integration proves stale fingerprints and denied describe audit. | PASS |
| No gate was weakened or bypassed. | No new lint suppression was introduced for the remediation; `check:clippy-allow-audit` is recorded passing; the mocks allowlist entries are confined to an existing `#[cfg(test)]` callback module and its dev dependency; failing Oracle timing tests were corrected with contract-derived deadlines rather than sleeps or ignored assertions. | PASS |
| Verification follows the capability task and exact-test rules. | Recorded evidence includes exact focused `nextest`/pytest/Vitest tests plus `verify:bifrost` 9/9, shared/Rust SDK, Python, TypeScript, codegen, client-tier, PyO3, format/lint, Clippy-allow, and diff checks. | PASS |
| Generated artifacts were regenerated, not treated as implementation authority. | JSON schemas, Python stubs, and N-API declarations have corresponding source changes; `codegen:check` and `ts:napi:check` are recorded passing, and no evidence shows hand-edited generated output. | PASS |
| R3 finding closure is complete. | FIND-4: root/nested hidden, array-extra, and symbol-key inputs are rejected; FIND-11: cited imports/signatures are corrected; FIND-16: both exported shapes are readonly aliases; FIND-17: evidence matches `WYRD_SPEC_400_VALIDATION`. | PASS |

## Material findings

None.

## Verification limits

- This was an independent static standards audit of the complete cumulative diff and surrounding owners; it did not rerun the expensive Cargo, Postgres, Python, or TypeScript suites.
- The candidate records `verify:bifrost` passing 9/9 at `d6231892`, after both Oracle timing-test corrections, and the final `b56560e5` commit changes only task evidence.
- The focused R3 TypeScript test, TypeScript unit/typecheck/integration lanes, queue fixed-binary tests, full observe unit suite, `fmt`, `lints`, `check:clippy-allow-audit`, and `git diff --check` are recorded passing; the broader unchanged R2 closure supplies shared SDK, Python, codegen, client-tier, PyO3, and N-API proof.
- `git diff --check c8bb490a..b56560e5` produced no whitespace error during this review.

## Overall result

**PASS** — every applicable repository standard reviewed for the cumulative candidate passes, R3 closes the four assigned standards gaps, and this audit proposes no material finding.
