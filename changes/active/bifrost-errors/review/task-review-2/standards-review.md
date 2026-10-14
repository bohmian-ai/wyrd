# Repository standards review

## Subject and coverage

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`7d6a1ff95b3dc09315e3af236ab9906b305e11d0`. HEAD still matched that
candidate at the end of source inspection. Reviewed the complete cumulative
diff, not merely the remediation commits. CodeGraph exploration of
`LiveScribeExec`, `LiveFragmentRead`, and `OracleSessionShape` preceded code
search; its source and caller map were expanded with the actual diff and
surrounding modules. No source, dependencies, prior reports, or test evidence
were modified.

This is a standards review, not a verdict on task acceptance. The supplied
implementation evidence is treated as recorded verification, not independently
rerun proof. No benchmark or Cargo lane was started.

## Authority coverage

The reference router `architecture/references/README.md` selects the following
applicable references, under AGENTS and agent-rules: doctrine
`architecture-constraints`; architecture `patterns`; languages `rust-core`,
`maintainer-style`, `spec-driven-development`, `implementation-execution`,
`testing-workflows`, `errors`, `python-api-and-stubs`, `typescript-guide`, and
`agent-harness`; domain `olap-serving`, `datafusion`,
`arrow-analytical-interop`, and `analytical-operations-reliability`. Governing
architecture inspection included Wyrd doctrine, Wyrd design's Bifrost/client
and error-catalog boundaries, Bifrost design's Oracle planning/distributed/
admission/terminal boundaries, and security posture's authorization,
peer-context, tenancy, and audit boundaries. No PyO3/N-API implementation,
generated surface, UI, persistent schema, or SQL query changed, so their
implementation-specific references do not add a separate write-set obligation.

| Changed surface | Applicable authority | Source inspection and result |
|---|---|---|
| `oracle/mod.rs` error mapping and mapper test | AGENTS §§3–6, 9–12, 16; agent-rules; errors, rust-core, OLAP serving; Bifrost terminal contract; security authorization/privacy | Existing `BifrostError` catalog and general mapper are reused. Logical diagnostics are admitted-table SQL diagnostics; physical planning retains fixed safe detail. Resources retain their original classification. No new error catalog, public shape, or transport mapping. **PASS**, except incomplete test panic documentation below. |
| `oracle/live.rs` leaf pushdown, resolution and lazy stream | AGENTS §§3–6, 10–12, 16; datafusion, Arrow interop, OLAP and reliability references; Bifrost planning and terminal contract | Stateful predicate/routes remain on existing `LiveScribeExec`; stream lifetime remains on existing `LiveFragmentRead`. Native recursive snapshot is reused. Static computation is synchronous; source IO is deferred to polling. `all_unsupported` retains authoritative operators. New production items describe snapshot timing, partition ownership, re-execution, error and drop behavior. **PASS** for standards. Function-local imports are a non-blocking note. |
| `resources.rs` session configuration | AGENTS §§3–6, 10, 16; datafusion; Bifrost admission/immutable planning | Two optimizer passes are set on the existing shared `OracleSessionShape::apply`, not a second planner or public knob. Surrounding consumers continue through `OracleSessionTemplate`/grant configuration. No change to runtime, batch size, spill ownership, memory ceilings, or query-class selection. Rustdoc explains the rule-order reason. **PASS**. |
| `Cargo.toml`, `Cargo.lock`, codec revision | AGENTS §§1–4, 11–12; architecture-constraints and DataFusion dependency-cone reference | Every workspace DataFusion patch pins `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`, as does `ORACLE_DATAFUSION_REVISION`; lock entries resolve the same fork universe. No Cargo feature, version wildcard, profile, client dependency, or distributed-fork revision was added. `wyrd-testing` retains DataFusion only in dev-dependencies. **PASS**. |
| `dispatcher.rs`, server `state.rs` and `peer_service.rs` observations | AGENTS §§3–6, 9–12, 16; patterns; test-support and reliability rules | Existing tally reports delivered rows; existing Scribe observation owner records exact counts and the last encoded fragment. Retained plan/row state is guarded by existing `test-support`; production signatures merely supply already-available values. Server remains the only listener. No new durable/public contract, audit writer, tenant authority, or governor. Methods/fields have substantive rustdoc. **PASS**. |
| Shared `query_fixture.rs`, `write.rs` and peer-cluster helpers | AGENTS §§3–6, 11–12, 16; testing-workflows, patterns, observation identity | Existing writer gains optional typed `RunId`; existing correlation and server stamping remain in use. Signature callers are adjusted; `write` explicitly passes None. Layout and fragment inspection extend existing `PeerCluster`, not an alternative cluster/runtime. New helpers have Errors documentation and explain their workflow. **PASS**. |
| New remote-live tests and module registration | AGENTS §§11–12, 16; agent-rules external-test placement; testing-workflows | Lives in the existing Oracle journey binary; module has inner rustdoc. Tests drive real public client/server/peer IO, use managed Postgres lane markers, and inspect exact footer/fragment observations. Tests and helpers document panic/error conditions. No test-only server state is exposed to SDK production packages. **PASS**. |
| Rust `query_bifrost`, `query_coverage`, integration registration and mise selector | AGENTS §§2–3, 11–12, 16; patterns, rust-core and testing-workflows | Real SDK journeys, fixed fixtures/expected answers, no secondary DataFusion oracle or new analytical dependency. New corpus remains one module in the established integration target; mise adds that module to the owning observe journey lane. New types, fields, helpers, tests and module registration have substantive rustdoc. **PASS**. |
| Python query journeys | AGENTS §§8, 11–12, 16; python-api-and-stubs, errors, testing-workflows | Public `wyrd` imports, top-level tests, real Python lifetime and existing server fixtures. Structured error code/detail assertions and typed helpers. No binding, export, generated stub or native implementation changed. **PASS**. |
| TypeScript query journeys | AGENTS §§2–3, 11–12; typescript-guide, errors, testing-workflows | Public facade, existing server fixture, Zod result projection and BigInt assertions. Fixed bounded cases, no transport or durable semantics duplicated. No N-API/declaration/generated artifact edit. **PASS**. |
| MCP query journeys | AGENTS §§9, 11–12, 16; agent-harness, errors, testing-workflows | Real mounted tool path, stable problem payload, sanctioned ignored Postgres journeys. No catalog or permissions change. New live-subquery test has Errors/Panics rustdoc. **PASS**, except materially expanded existing error journey's missing Panics section below. |
| Spec and prior review/evidence documents | AGENTS §§14, 16; spec-driven-development and implementation-execution | Packet stays in normal tracked `changes/active`; review facts do not become runtime code/configuration. No generated artifact edited. One committed whitespace failure is recorded below. **PASS** for architectural boundaries; evidence limits apply. |

## Rule results

| Rule | Result and evidence |
|---|---|
| Narrow owner and dependency placement | PASS: Vala owns query computation, server owns peer serving/test observations, SDKs only exercise client surfaces, test fixture remains `wyrd-testing`. |
| Single planner/root, no fallback/allowlist | PASS: no new physical planner, classifier, SQL parser or operator allowlist in diff; the shared session config and native filter APIs are extended. |
| Truthful pushdown and maintained final authority | PASS: `live.rs:444–477` reports unsupported propagation and keeps producing operators above the leaf. Semantic completeness is independently assessed by domain/implementation reviews. |
| TenantConn/OperatorPool and audit invariants | PASS/N/A to writes: no new pool/signature/SQL/audit authority. Existing admitted bindings/context are retained before dispatch and IO. No parallel per-row tenancy check or audit writer added. |
| Async earns IO, no ad hoc runtime | PASS: classification, snapshot and pruning stay synchronous; new async fixture/query helpers await real ingest/register/query IO. |
| Stable errors and source-derived contracts | PASS: existing derive-backed errors reused; no stub/schema/declaration or catalog fork. |
| Test runtime and tier placement | PASS: Rust-only snapshot logic inline, cross-crate/server tests in existing journey targets, language lifetime tests in owning language. |
| No weakened gates | PASS: new ignores are sanctioned environment-owned journey markers and mise runs ignored tests. No deleted failing test, new allow, retry/sleep relaxation, timeout extension or boundary glob. |
| Complete rustdoc including panic conditions | FAIL: two materially changed test items omit mandated Panics documentation (STD-R2-1). Other changed owners/helpers have substantive docs. |
| Imports at module top and bare imported signature types | Minor deviations: new scalar snapshot test imports at function scope (`live.rs:1080–1094`); new fixture helpers use fully-qualified type paths in signatures (`query_coverage.rs:305`, `peer_cluster.rs:526`). No demonstrated behavior/public-contract consequence; non-blocking. |

## Proposed repository-rule finding

### STD-R2-1 — Incomplete panic documentation on materially changed test items

- Classification: VIOLATION; bounded documentation correction.
- Obligation: AGENTS §16 requires `# Panics` whenever a panic remains possible;
  `languages/rust-core.md` Documentation explicitly treats incomplete rustdoc
  as a hard blocker. The task-review skill preserves the rustdoc gate while
  requiring documentation corrections to be batched with substantive fixes.
- Locations: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4473–4477`,
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:202–214`.
- Evidence: cumulative diff substantially rewrites the mapper test and expands
  the MCP error journey. The mapper doc has only a behavior paragraph; MCP doc
  has Errors but no Panics. Their bodies assert expected classification,
  suggestions and non-disclosure, so assertion failures intentionally panic.
- Consequence: fails the repository's explicit documentation completion gate;
  not a runtime defect. Existing intent docs should be kept; only the missing
  failure condition needs documenting. The prior standards report's claim that
  every changed test documented panics is not supported by current source.
- Smallest correction: add concise Panics sections to those two touched test
  items identifying failed error-mapping/repair-detail/security assertions. No
  behavior, new fixture, gate, or test change.
- Closure: source inspection plus format check; batch with any validated
  implementation correction rather than create a style-only remediation round.

## Verification evidence and limits

Committed remediation evidence records DataFusion snapshot tests, focused Wyrd
unit tests, exact remote journeys, common corpus/query_bifrost journeys, Oracle
57/57, fmt/lints/codegen/dependency/tenancy checks. Source selectors and targets
match current registered tests. No broad runtime lane was rerun here by design.
Python/TS source is unchanged from the previously reviewed candidate; their
earlier journey evidence remains relevant, although the new shared fork/config
must receive integrated verification at change review.

The orchestrator observed cumulative `git diff --check 526355fb8..7d6a1ff95`
failing on `changes/active/bifrost-errors/review/task-review-1/verdict.md:85`
(new blank line at EOF). Plain working-tree `git diff --check` does not prove
the committed range. This is a compact non-blocking documentation note, not
another runtime finding or a basis for redesign.

An earlier Oracle lane reserved-memory assertion failure is recorded without
diagnosis, followed by a passing run. This standards audit does not invent a
cause or establish recovery proof from that later pass; system/implementation
reviews must reconcile that evidence. No benchmark claim is made.

## Overall result

**FAIL** for the explicit incomplete-rustdoc gate only (STD-R2-1).
Production/test ownership, language/runtime boundaries, manifest placement,
error-contract projection, and verification wiring otherwise conform. Final
classification of this documentation proposal belongs to independent validation.
