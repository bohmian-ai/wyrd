# TASK-002 Repository Standards Review

## Immutable Subject

- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `fbfc2591a985b288935180098f892aecdf3b8b49`
- Range: complete `base..candidate` diff (97 files, including committed task and prior-review artifacts)
- Candidate was `HEAD` before and after review inspection. No reviewed source file was modified.

## Authority Coverage

| Changed surface | Applicable authority read | Rule result and exact evidence |
|---|---|---|
| Repository-wide Rust, manifests, task packet, and generated artifacts | `AGENTS.md` §§2–12, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md` | **FAIL.** The implementation follows the required owner split and recorded verification is broad, but new Rust modules lack required rustdoc (RS-005), new code uses forbidden local/fully-qualified imports (RS-006), and required concurrency/backpressure/error-path proofs are absent for reachable defects (RS-001, RS-002, RS-004). |
| Wyrd protocol, client model, observation identity, and public surface alignment | `architecture/wyrd-design.md` (Doctrine, Client model, Verifier, Bifrost); `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md` | **FAIL.** `wyrd-client` remains the single Rust owner and durable identity fields remain server-managed, but the Python/TypeScript active-span promise is not implemented at either foreign-runtime boundary (RS-003). |
| Shared Rust client: `WyrdState`, scoped runs, observation projection, Bifrost facade/cache | `AGENTS.md` §§3–6, 9–11, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md`; `architecture/references/domain/telemetry-observations.md`; `architecture/references/domain/analytical-operations-reliability.md` | **FAIL.** Struct ownership and bounded queue reuse pass (`WyrdState` owns `BifrostLifecycle`; `Bifrost` owns `WriterPool`), but start/shutdown is not linearizable (RS-002), multi-row Drift enqueue can partially admit one logical observation (RS-004), and import/documentation hard rules fail (RS-005, RS-006). |
| `wyrd-queue` JSON-to-Arrow `FixedSizeBinary` support | `AGENTS.md` §§4–6, 11, 16; `architecture/references/languages/rust-core.md`; `architecture/references/domain/arrow-analytical-interop.md`; `architecture/references/domain/analytical-operations-reliability.md` | **PASS.** Width is schema-driven, decoding is checked, malformed/wrong-width hex is refused, and `batch_builder_tests::fixed_size_binary_*` exercises 16-byte and 8-byte identities. Recorded focused test and `test:shared` passed. |
| Vala built-in table schemas, catalog materialization, cache/Forge consumers | `architecture/bifrost-design.md` (Table and row identity, Resource invariants, Public surface); `architecture/references/domain/vala-architecture.md`; `domain/olap-serving.md`; `domain/iceberg.md`; `domain/datafusion.md`; `domain/arrow-analytical-interop.md`; `domain/evaluation.md`; `domain/drift-monitoring.md` | **FAIL.** The five definitions use the canonical table registry, typed Arrow fields, daily layouts, and managed correlation, and schema tests exist; however the new table modules violate the repository's mandatory rustdoc rule (RS-005). No positional schema mapping or alternate warehouse/client was introduced. |
| Public errors and cross-language projection | `AGENTS.md` §4 and §9; `architecture/references/languages/errors.md`; `architecture/references/architecture/patterns.md` | **PASS.** New public lifecycle/observation failures are derive-backed `WyrdError` variants in `crates/wyrd-spec/src/error.rs`; Python and TypeScript route native metadata through their existing central error projection. Recorded `codegen:check`, Python and TypeScript suites passed. |
| `wyrd-spec` Eval/Drift contracts and generated JSON schema fixtures | `AGENTS.md` §§2–4, 8–10, 12; `architecture/wyrd-design.md` §Verifier; `architecture/references/doctrine/architecture-constraints.md`; `domain/evaluation.md`; `domain/drift-monitoring.md`; `languages/errors.md` | **PASS.** The crate remains IO/async/PyO3-free; record identity changes are typed and generated fixtures were regenerated. Recorded `codegen:check` and `check:client-tier` passed. |
| Python SDK package, PyO3 wrappers, stubs, and tests | `AGENTS.md` §§7–8, 11; `architecture/references/languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `languages/testing-workflows.md`; `domain/telemetry-observations.md` | **FAIL.** Placement, public imports, stubs, and real-server journey coverage pass; no `Bound` crosses an await and the production wheel excludes testing. The claimed Python active OpenTelemetry fallback never reads Python's active context (RS-003), and one new signature uses a forbidden fully-qualified type (RS-006). |
| TypeScript SDK source, N-API wrapper, generated declarations, and tests | `AGENTS.md` §§2–3, 11; `architecture/references/languages/typescript-guide.md`; `languages/testing-workflows.md`; `languages/errors.md`; `domain/telemetry-observations.md` | **FAIL.** N-API remains a thin wrapper over `wyrd-client`, declaration/typecheck/N-API checks and a real-server journey are recorded passing, but the claimed JavaScript active-span fallback is not bridged into Rust (RS-003). |
| Server, test server, authentication scope fixture, and audit publisher lifecycle | `AGENTS.md` §§2, 6, 9, 11–12; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `domain/analytical-operations-reliability.md`; `architecture/operations/README.md`; `architecture/operations/reliability-and-recovery.md` | **FAIL.** Production still owns listeners/auth/tenant state and the audit-disable switch is `test-support`-only; however the new audit lock-timeout recovery returns success from an aborted PostgreSQL transaction (RS-001). |
| Vala SQL audit and Forge task timing | `architecture/agent-rules.md` SQL/audit rules; `architecture/wyrd-security-posture.md` §§Tenant isolation, Audit integrity; `architecture/bifrost-design.md` §§Read audit, Forge; `architecture/operations/reliability-and-recovery.md`; `domain/analytical-operations-reliability.md` | **FAIL.** SQL stays behind `TenantConn`/`OperatorPool`, callees do not commit, and Forge immediate eligibility now uses the database clock. The audit timeout branch does not leave its caller a committable transaction (RS-001). |
| CLI/eval test-support adaptations and Rust SDK journey | `AGENTS.md` §§3, 11–12; `architecture/references/languages/testing-workflows.md`; `domain/evaluation.md` | **PASS.** Changes are test-consumer adaptations; the Rust SDK journey drives real SDK → server → Bifrost → query and is in the capability gate. |
| Tooling: `mise.toml`, check scripts, workspace-hack, lockfile | `AGENTS.md` §§1, 4, 11–12, 15; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `languages/implementation-execution.md` | **PASS.** Changed checks now use the pinned `uv` interpreter; the mocks allowlist is limited to a `#[cfg(test)]` module/dev dependency and its self-described sanctioned case. `verify:bifrost`, formatting/lints, workspace-hack generation, and boundary checks are recorded passing. |
| Architecture/task documentation | `AGENTS.md` §§2, 14–15; `architecture/references/README.md`; `languages/spec-driven-development.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` | **PASS.** The architecture now states lazy built-in describe and the valid caller-owned namespace. Task/review records remain change artifacts rather than code comments or runtime contracts. |

## Applicable Rule Results

| Repository rule | Result | Evidence |
|---|---|---|
| One shared `wyrd-client::Bifrost` composition; no second queue/transport/client | PASS | `crates/shared/wyrd-client/src/state.rs`, `src/observe/*`, and all language wrappers call the same `Run`/`Observe`/`Bifrost` owner. |
| Stateful workflows live on cohesive concrete owners | PASS | `WyrdState`, `BifrostLifecycle`, `Run`, `Observe`, `Bifrost`, and `WriterPool` own their respective state/dependencies. |
| Async is limited to actual IO/composition | PASS | `record*`, startup, flush, and shutdown await describe/transport/drain; Drift/Eval parsing/projection remain synchronous. |
| Tenant, Card, Run, publisher, and managed-column identity remain typed and server-authoritative | PASS | `RunId`, `CardRef`, `CardUid`, `DataTenantId`, managed correlation, and Gate/Scribe stamping remain separate; user rows do not author `card_ref`/`run_id`. |
| Every external/runtime surface projects the same observation behavior | FAIL | RS-003. |
| Shutdown and cancellation close admission and cannot report clean completion while work may subsequently start | FAIL | RS-002. |
| Bounded queues fail without silently leaving a partial logical observation | FAIL | RS-004. |
| PostgreSQL failure paths remain explicit and leave transaction lifecycle coherent | FAIL | RS-001. |
| Every new/materially modified Rust item has complete rustdoc | FAIL | RS-005. |
| Imports live at module top and signatures use imported bare types | FAIL | RS-006. |
| Public stable errors use the derive-backed catalog | PASS | New SDK errors in `crates/wyrd-spec/src/error.rs`; recorded code generation passed. |
| PyO3 stays in the Python SDK and uses the shared runtime bridge | PASS | New wrappers are under `sdks/wyrd-sdk-python/src`; `check:pyo3-scope` passed. |
| Python exports/stubs/runtime imports agree | PASS | `codegen:check`, `py:typecheck`, unit and integration suites recorded passing. |
| TypeScript wrappers/declarations/N-API agree | PASS | `ts:typecheck`, `ts:napi:check`, unit and integration suites recorded passing. |
| User-facing Rust/Python/TypeScript capability has real-server journeys | PASS | `observe_run.rs`, `test_observe_journey.py`, and `observe-run.test.ts`; all included in `verify:bifrost`. |
| Generated artifacts are source-derived and drift-checked | PASS | Recorded `codegen:check`; no uncommitted generated drift after the review's targeted checks. |
| Test-only controls cannot enter production profiles | PASS | `audit_publication_disabled` is guarded by `feature = "test-support"`; `check:py-wheel-no-testing` passed during review. |
| No gate was disabled or weakened to hide production behavior | PASS | `check:mocks-scope`, `check:unwrap-audit`, and `check:clippy-allow-audit` passed during review; the new mock allowlist covers only `#[cfg(test)]` code and a dev dependency. |

## Material Findings

### RS-001 — Lock-timeout handling returns `Ok(None)` from an aborted transaction

- **Rule:** `AGENTS.md` §§4, 6, 12 and `architecture/operations/reliability-and-recovery.md` require explicit failure handling, bounded dependency waits, and coherent retry/recovery state; `architecture/agent-rules.md` requires caller-owned `TenantConn` transaction lifecycle.
- **Location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:239-272`; consumer `crates/wyrd/wyrd-server/src/audit/publication.rs:359-365`.
- **Evidence:** `SET LOCAL lock_timeout = '3s'` causes the `FOR UPDATE` statement to return PostgreSQL `55P03`. PostgreSQL marks the transaction failed after that statement error. The new branch at line 271 converts the error to `Ok(None)` without a savepoint rollback, after which the publisher unconditionally calls `conn.commit()`.
- **Consequence:** A held chain-head lock does not produce the documented idle cycle. It produces a later commit failure from an already-aborted transaction, makes the SQL API return apparent success to any caller that has not yet committed, and misclassifies the actual failure boundary. No new test holds the lock beyond the timeout and commits the returned connection.
- **Testable correction:** Preserve the 3-second bound but make the outcome transactionally valid: either propagate the lock-timeout as the explicit retryable SQL failure, or contain the statement in a savepoint and roll back to it before returning `None`. Add a Postgres test that holds `vala.audit_chain_head` past the timeout, asserts the exact chosen outcome, proves the transaction can be completed according to that outcome, and proves a later publisher retries the same tenant without changing a frozen bound.

### RS-002 — Concurrent shutdown can return success while startup later publishes a live writer

- **Rule:** `AGENTS.md` §6 and `architecture/references/domain/analytical-operations-reliability.md` require structured lifecycle/cancellation; shutdown must close admission and drain owned work before reporting success.
- **Location:** `crates/shared/wyrd-client/src/observe/lifecycle.rs:91-103`, `134-141`, and `171-186`.
- **Evidence:** `claim()` moves the phase to `Starting` and releases the mutex during describe IO. `shutdown()` calls `started()` and treats every error as a successful no-op. Therefore a shutdown racing a claimed start observes `Starting`, returns `Ok(())`, and the still-live `StartClaim::complete` later writes `Phase::Started`.
- **Consequence:** A caller can receive a successful durability barrier while the same `WyrdState` subsequently opens a writer and accepts rows. This violates the stated one-lifetime shutdown contract and can strand data/resources during application teardown.
- **Testable correction:** Make `Starting` a distinct shutdown outcome that cannot be reported as successful while the claim may still publish (wait, cancel/fence, or return a stable in-progress refusal within the existing lifecycle owner). Add a deterministic concurrent test that pauses startup between claim and completion, invokes shutdown, releases startup, and proves that any successful shutdown leaves the state permanently closed with no started writer.

### RS-003 — Python and TypeScript promise active-span fallback without bridging their runtime contexts

- **Rule:** `AGENTS.md` §§2–3 and `architecture/references/architecture/patterns.md` require all first-class SDKs to project the same behavior; `domain/telemetry-observations.md` requires typed context propagation rather than process/thread globals; PyO3/N-API boundaries must convert foreign-runtime inputs at the edge.
- **Location:** shared fallback `crates/shared/wyrd-client/src/observe/eval.rs:192-214`; Python boundary `sdks/wyrd-sdk-python/src/observe/mod.rs:178-202` and public stub `python/wyrd/observe/__init__.pyi:83-87`; TypeScript boundary `sdks/wyrd-sdk-ts/native/src/cards.rs:466-498` and public wrapper `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1114-1132`.
- **Evidence:** Both foreign boundaries pass `None` trace/span values directly to `EvalObservationOptions::from_parts`. The only fallback reads `tracing::Span::current()` in Rust. No Python `opentelemetry.trace.get_current_span()` or JavaScript `@opentelemetry/api` active context is read or propagated across PyO3/N-API; repository search finds no such bridge. Existing Python/TypeScript tests cover explicit IDs only, while their public docs say the active foreign-runtime span supplies both IDs.
- **Consequence:** Eval observations emitted inside an active Python or JavaScript OpenTelemetry span silently store null `trace_id`/`span_id`, breaking the documented correlation join while Rust callers behave differently.
- **Testable correction:** At each foreign-runtime boundary, read that runtime's installed OpenTelemetry active span when both IDs are omitted and pass its validated trace/span IDs into the existing Rust-native options path. Add Python- and TypeScript-runtime tests that create an active valid span, call `eval` without explicit IDs, and assert both exact IDs in the projected/persisted row; retain explicit-ID precedence and span-without-trace refusal tests.

### RS-004 — Multi-row Drift emission can partially enqueue one observation

- **Rule:** `AGENTS.md` §§6, 10–11 and `architecture/references/domain/telemetry-observations.md` require bounded admission and explicit incomplete-ingest failure; `domain/analytical-operations-reliability.md` requires accepted ownership and backpressure transitions to remain coherent.
- **Location:** `crates/shared/wyrd-client/src/observe/mod.rs:161-174`; per-row admission `crates/shared/wyrd-client/src/bifrost/handle.rs:176-186` and `crates/shared/wyrd-queue/src/producer.rs:661-694`.
- **Evidence:** One Drift call first projects all feature rows, then enqueues them one at a time. Each `enqueue` independently reserves bytes and uses `try_send`; any later row can return queue-full after earlier rows have already been accepted. The method returns `Err`, but the accepted prefix remains buffered and is later durable. No saturation test exercises a failure after the first feature row.
- **Consequence:** Retrying after the error can duplicate the accepted prefix, while not retrying persists an incomplete feature set for one `record_id`. Drift analysis then consumes a record the caller was told was not accepted.
- **Testable correction:** Admit or reject every row projected from one Drift observation as one logical bounded enqueue before mutating the producer, while retaining the existing asynchronous durability boundary. Add a minimal-capacity test that forces insufficient room for all feature rows and proves zero rows from that `record_id` are later flushed; also prove the all-fit case retains every feature exactly once.

### RS-005 — New Vala table modules do not satisfy mandatory rustdoc coverage

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` make rustdoc for every new or materially modified Rust module/item a hard pre-merge requirement.
- **Location:** `crates/vala/vala-bifrost-redux/src/tables/drift/result_features.rs:1`; `tables/eval/mod.rs:1-5`; `tables/eval/observations.rs:1`; `tables/eval/result_items.rs:1`; `tables/verification/mod.rs:1-3`; `tables/verification/results.rs:1`; parent declarations `tables/mod.rs:18,25`.
- **Evidence:** All six files are new and begin with imports or bare `mod` declarations rather than module rustdoc. The new `eval`/`verification` module declarations and their private child declarations/re-exports have no item documentation. Compile and Clippy success does not enforce this repository-specific hard rule.
- **Consequence:** The candidate fails the repository's explicit completion standard even though the table structs themselves have useful comments.
- **Testable correction:** Add intent/workflow/invariant rustdoc to every new module declaration/file and any otherwise undocumented new item in those modules. Verify with a source audit of all added Rust items plus the existing format/lint lanes; do not add lint suppressions.

### RS-006 — New code hides dependencies in functions and uses fully qualified signature types

- **Rule:** `architecture/agent-rules.md` requires all `use` statements at module top and imported bare types in signatures, with only the stated test-module and single-generic-trait exceptions.
- **Location:** `crates/shared/wyrd-client/src/observe/eval.rs:198-200`; `crates/vala/vala-bifrost-redux/src/tables/mod.rs:721-723`; `crates/shared/wyrd-client/src/observe/lifecycle.rs:63-65`; `sdks/wyrd-sdk-python/src/observe/mod.rs:17`.
- **Evidence:** `active_span_identity` imports two traits inside the function; the table test helper imports field constructors inside the function; `BifrostLifecycle` spells `std::fmt::*` in its impl/signature; `invalid_argument` spells `impl std::fmt::Display`. None matches the allowed single-generic-function trait exception.
- **Consequence:** The candidate violates an explicit repository dependency-visibility rule across production and test code.
- **Testable correction:** Move the imports to each module's top-level import block and use bare imported `Debug`, `Formatter`, `Result` alias as appropriate, and `Display` in the signature. Run formatting and lints and re-audit the complete added-line set for function-scoped imports and fully-qualified signature paths.

## Verification Notes

Recorded candidate evidence reports these passing: focused queue regression, `test:shared`, `test:wyrd-sdk`, all nine `verify:bifrost` lanes, Rust/Python/TypeScript journeys, Python unit/integration/typecheck, TypeScript unit/integration/typecheck/N-API check, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, Rust/Python formatting and lints, and `git diff --check`.

This review additionally ran successfully against the candidate:

- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:mocks-scope`
- `mise run check:py-wheel-no-testing`

The recorded suites do not exercise the lock-timeout commit path, the start/shutdown race, partial Drift admission under saturation, or actual Python/JavaScript active OpenTelemetry contexts. Those are the focused closure proofs required by RS-001 through RS-004.

## Overall Result

**FAIL**

Authority coverage is complete, but six material repository-rule findings remain. RS-001 through RS-004 are reachable correctness/reliability or cross-runtime contract defects; RS-005 and RS-006 are explicit hard repository-standard violations.
