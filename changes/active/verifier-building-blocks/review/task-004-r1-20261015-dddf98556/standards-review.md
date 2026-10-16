# Independent repository standards review

Result: **FAIL** — one proposed rustdoc-completeness finding, subject to independent validation.

## Subject and limits

Reviewed base `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` through candidate `dddf985563029a004fa768110e36d0c5f12bcded`, with the prior-candidate-to-HEAD diff used to locate R1 changes. `subject.md` supplies the immutable identities and human decisions. No peer discovery report was read. CodeGraph was used before code searches. No production source, generated artifact, or prior review was changed. No Cargo command, full journey, or gate was run by this reviewer.

The owner's approvals apply: extra fixture validation and PDF attributes are authorized; r8 supersedes FIND-004-7; journey execution waits for TASK-005; provisional `code.run` arguments are not a defect. This report audits repository rules, rather than deciding original task acceptance.

## Authority coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, the reference router, and `languages/spec-driven-development.md`, `languages/implementation-execution.md`, and `languages/maintainer-style.md` under `architecture/references/`.

| Changed surface | Additional governing authority | Source coverage |
| --- | --- | --- |
| Shared client observe, state, verification, storage and artifact hash reuse | `wyrd-design.md` Doctrine/Client model/Verifier; `wyrd-doctrine.mdx`; references `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `domain/evaluation.md`, `domain/telemetry-observations.md`, `domain/drift-monitoring.md` | `observe/{mod,tests,lifecycle,drift,verify}.rs`, removed Eval projection, `state.rs`, `verify/{mod,execution}.rs`, `storage/{mod,upload/mod}.rs`, `cards/saga/hash_artifacts.rs` and callers |
| Python wrappers, package, authored stubs and tests | References `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`, `languages/errors.md`, `languages/testing-workflows.md`; Client model | SDK `src/{client,verify,observe/mod,lib}.rs`, `python/wyrd/{verify,observe,__init__}*`, removed Eval exports, type fixture, public export and Observe unit tests; integration support/provider doubles and journeys |
| TypeScript native/ergonomic interfaces and generated declarations | References `languages/typescript-guide.md`, `languages/errors.md`, `languages/testing-workflows.md`; Client model | Native `cards.rs`, `verification.rs`, `lib.rs`; public `src/index.ts`, declarations, generated Card/error types and relevant unit/type/journey/support files |
| Rust SDK and three-language examples | Client model, references `architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md`, `domain/evaluation.md` | Rust re-exports/manifest, support-desk entrypoints and policy/reader/judge Cards, all three journey families, retained existing-journey callers |
| Fixtures and staged validation | Verifier and reference-slot design; references `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `domain/evaluation.md`, `domain/drift-monitoring.md`, `languages/testing-workflows.md` | Verifier building-block Cards/assets, scheduled PSI/SPC Cards, existing rewritten fixture families, `wyrd-spec/tests/verifier_fixtures.rs` |
| Error/schema/type generation | `AGENTS.md` §§4,8,9; references `languages/errors.md`, `languages/agent-harness.md` | `wyrd-spec/src/error.rs`, schema generator, `scripts/gen_ts_types.py`, generated TS files |
| Analytical reads and identity in test consumers | `bifrost-design.md` Table and row identity/Storage format and Variant/Public surface; references `domain/olap-serving.md`, `domain/telemetry-observations.md` | Scheduled result readers and attribution readers in three SDKs; Bifrost source/docs edits; server/MCP test caller migrations |
| Authorization examples and hosted-tool assertions | `wyrd-security-posture.md` Security principles/Trust boundaries/Authorization/Tenant isolation/Audit/Secret handling; Verifier Agents/tools section; references `languages/agent-harness.md` | Exact typed grants, delegate attenuation and retained actor/subject assertions, no-data-on-denial assertions, reference guide |
| Documentation, architecture prose, diagrams and landing copy | Current design/doctrine, applicable evaluation/drift references; `AGENTS.md` §§2,9,11 | Changed reference pages, verification/observe guides, understand pages, quickstart, diagram exports and landing-page text; protocol vocabulary changes |
| Tooling and provenance | `AGENTS.md` §§11,14,16; spec-driven development and testing references | `mise.toml` scoped journey selection, Python lint exclusion removal, Cargo dependency/lock changes, `.gitattributes`, approved packet/review provenance |

No Bifrost execution/storage algorithm, SQL capability signature, deployment topology, or server listener is added in this range. Deep Iceberg/DataFusion/Arrow execution references are consequently not applicable; architectural and query identity rules above are applicable.

## Rule results and evidence

| Rule | Result | Exact authority and source evidence |
| --- | --- | --- |
| Core durable behavior stays server-owned; clients use one shared implementation | PASS | `AGENTS.md` §§2–3; `patterns.md` Client Pattern. `Observe::verify/submit` route via shared `Target`; PyO3 and napi convert inputs and wrap shared `Execution`; no SDK result store, worker or durable queue is added. |
| Stateful Rust capabilities have meaningful concrete owners; pure conversion stays synchronous | PASS | `AGENTS.md` §§5–6. `Execution` owns ID/client; `Target` owns one invocation's target references; `RunEvidence` owns pinned declarations/uploads; Python reuses existing `LazyConnection`; slot converters are synchronous. |
| Reuse storage transfer/hash owner rather than creating another uploader | PASS | `AGENTS.md` §§3,15. Verification calls shared stream hash and `WyrdStorageClient::upload_evidence`; evidence transfer dispatches through existing upload mechanisms. |
| Typed IDs and shared stable error catalog | PASS | `AGENTS.md` §4; `languages/errors.md`. Execution uses `ExecutionId`; new timeout is derive-backed `WyrdError::SdkVerificationWaitTimedOut`; Python/napi retain catalog projection. |
| SQL capability types and transaction ownership | PASS / no changed production SQL | Agent rules ban raw pools in fields/signatures and callee commit. Complete Rust diff adds no SQL pool/connection/transaction field or signature. New analytical consumers call public Bifrost, not `PgPool`; no tenant SQL authority is moved into SDKs. |
| Tenant/publisher authority remains credential-derived | PASS for changed SDK ownership | Security posture and `AGENTS.md` §9. Inputs carry subject references, not a trusted tenant override; authenticated shared client performs requests. New tool tests grant capabilities through real harness principals and inspect retained attribution. Runtime enforcement is TASK-005 proof, not claimed here. |
| Python wrappers reside at runtime boundary, use shared runtime, no `Bound` across awaits | PASS | `AGENTS.md` §§7–8; PyO3 reference. New wrappers are under Python SDK, convert before shared Rust calls, and use existing trace/block-on bridge. No Python lifetime is initialized in a Rust test. |
| Python public exports/native registration/stubs agree; fixed records are typed | PASS for inspected declarations | `AGENTS.md` §8; Python Stub Rules. Verify classes are registered and publicly re-exported; authored stub uses `TypedDict`, tagged unions and JSON leaves; `verify_contracts.py` checks accepted shapes and invalid fixed keys/types. Independent `py:typecheck` passes in `verification.md`. |
| Python lazy/eager construction reuses retained connection owner | PASS | Design Client model and Python Code Style. `PyExecution::__new__` only parses ID and retains optional client; `connect` calls `handle`; queued construction uses `LazyConnection::ready`. Lazy/eager boundary tests are Python tests. |
| TypeScript native conversion retains slot distinction and precise public return types | PASS | TS API Shape/Napi Bridge. `nativeInputs` distinguishes plain named maps and explicit positional objects; `NativeVerifierInput.slot` is optional and shared `Inputs::One/Named` retains distinction. `verify/submit/status/wait` declare wire result types. |
| Generated outputs have owning source changes | PASS for source provenance; drift check not independently rerun here | Agent rules Generated Artifacts. Rust napi input source and checked-in declarations change together; schema root/generator changes explain `VerificationResult`/execution-state generated types; authored Python stubs are correctly separate. |
| Scheduled consumers use published execution identity | PASS; FIND-004-6 source closure | Bifrost Storage format and Variant names `execution_id`. Shared scheduled readers use `SELECT execution_id`; Rust/Python/TS attribution readers filter `execution_id`, and handle reconstruction uses that value. |
| Assets are normally committed and match advertised extraction coverage | PASS for delivery; FIND-004-4 source closure | All nine `samples/sample.*` assets exist in the candidate tree; format fixture and journey consumers use them. PDF binary attributes are explicitly approved. |
| Tests use owning runtimes and gated real-server journeys | PASS for structure; execution deferred | `AGENTS.md` §11; Testing Workflows. Rust journeys use existing managed-server lifecycle/ignore policy; Python integration functions use public imports and fixtures; TS tests use owning Node harness. Existing mise journey families include the new files. New fixture validator's location is explicitly approved. |
| No test-integrity bypass | PASS | Agent rules and §12. New ignores mark real Postgres journeys using established lane policy, not a test failure workaround; no production validation gate is disabled. |
| Changed Rust rustdoc complete, including helpers/tests | FAIL: STD-R1-001 | `AGENTS.md` §16 and agent-rules Rust documentation. Original missing wrapper-field documentation is fixed, but newly added validator helper/test omit required panic sections. Details below. |
| Vocabulary and documentation preserve revised contract | PASS for standards | Doctrine and Verifier section: named inputs, preparation/check separation, Run vs Window, queued-only failed dispatch, exact identities and non-blocking audit appear consistently in changed principal guides/examples. Provisional runtime/MCP completion is not represented as independently executed proof. |
| Narrow verification scope | PASS with explicit limits | Owner decision and `languages/testing-workflows.md`. `verification.md` records Rust integration compile/discovery, fixture test, scoped all-feature library Clippy, Python typecheck/collection and TS typecheck/discovery. Wiremock execution is sandbox-blocked before behavior; no passing deadline assertion is inferred from compilation. |

## Proposed finding

### STD-R1-001 — VIOLATION: new fixture-validator panic contracts are undocumented

- **Rule:** `AGENTS.md` §16 requires rustdoc for every changed item, expressly including private helpers and tests, and says “Add `# Panics` whenever a panic remains possible.” Agent rules make missing/placeholder rustdoc `BLOCK_BEFORE_MERGE`; the review skill preserves this specific blocker despite the ordinary style threshold.
- **Locations:** `crates/wyrd-spec/tests/verifier_fixtures.rs:171–175` (`sibling`) and `:187–205` (`fixture_verifiers_satisfy_the_staged_contract`).
- **Evidence:** `sibling` calls `card.parent().expect(...)` but has only a one-line summary. Its callers in binding collection and Agent/Prompt resolution rely on the parent-directory invariant. The test asserts nonempty collections and explicitly panics when fixture validation or a binding lookup fails, but has no `# Panics`. Neighboring new helpers `decode`, `canonical`, `Fixtures::load/add/collect_bindings/resolve` already document these contracts.
- **Consequence:** The approved extra fixture gate is behaviorally present, but does not meet the explicit required documentation gate for materially added Rust items. This is not a claim that adding that test was out of scope, nor a proposed runtime behavior change.
- **Smallest correction:** Add substantive `# Panics` sections naming `sibling`'s parent-path invariant and the fixture test's loading/resolution/validation/assertion failure conditions. Preserve the validator and its assertions. No new utility, dependency, architecture choice or test is required.
- **Closure proof:** Inspect these item docs and run only formatting/owning lint checks if the remediation changes Rust. The existing exact fixture test already supplies behavioral proof; there is no reason to rerun journeys for a doc-only correction.

## Prior standards-related findings

- **FIND-004-4:** committed assets close the missing-source aspect; extraction behavior remains the deferred server journey obligation.
- **FIND-004-6:** all six scheduled/attribution identity consumers inspected now use the canonical execution column.
- **FIND-004-8:** fixed Python dictionaries now have precise authored declarations and a positive/negative type fixture; broad JSON remains only on open payload leaves and declared open check metadata.
- **FIND-004-9:** both originally cited wrapper ownership fields now carry rustdoc. STD-R1-001 is a new completeness issue in new R1 code, not a claim those fields remain unfixed.
- **FIND-004-7:** superseded by approved r8 and excluded from reopening.

## Non-blocking notes

Some newly touched Rust code retains qualified type paths and function-scoped imports (for example the new wiremock deadline test). Agent rules prefer top-level imports and bare imported signature types. These have no demonstrated behavioral, contract or maintenance-safety consequence in this review and do not block acceptance under the skill threshold. No optional refactor is requested.

Skipped full journey execution and broad gates as directed. The only proposed blocker above is the explicit Rust documentation requirement; compiled/discovered journeys still need merged-change execution after TASK-005.
