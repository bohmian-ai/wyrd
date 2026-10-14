# Repository standards review — round 3

## Immutable subject and method

Wyrd base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`, candidate
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`; DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, pinned candidate
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`. Distributed fork remains
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Inspected the cumulative Wyrd and dependency diffs, using the latest remediation
diff only for navigation and prior-finding closure. CodeGraph exploration of
`LiveScribeExec`, `caller_planning_fault`, and the new journey preceded source
search. Expanded its map through owning modules, callers, language journeys,
manifest/lock entries, codec revision, test registration and committed evidence.
The candidate stayed read-only; only this report was written. This pass assesses
repository standards rather than independently deciding task acceptance or SQL
semantic completeness.

## Authority coverage

Read AGENTS and agent-rules, then selected through the reference router:
`doctrine/architecture-constraints`, `architecture/patterns`; languages
`rust-core`, `maintainer-style`, `spec-driven-development`,
`implementation-execution`, `testing-workflows`, `errors`,
`python-api-and-stubs`, `typescript-guide`, `agent-harness`; domain
`datafusion`, `olap-serving`, `arrow-analytical-interop`, and
`analytical-operations-reliability`. Governing anchors include Wyrd doctrine,
Wyrd design's client model, Bifrost design's immutable planning, distributed
execution and terminal contracts, and security posture's authorization,
peer-context, tenant and audit boundaries.

The fork is an upstream-style DataFusion dependency, not Wyrd source relocated
outside its crate. Its changes are assessed against native AggregateUDF,
Accumulator, optimizer, function registration, proto and documentation
conventions; Wyrd-specific per-item rustdoc formatting is not imposed on
unmodified upstream trait implementations or generated macros.

| Changed surface | Governing authority | Evidence and result |
|---|---|---|
| `oracle/mod.rs` logical/physical/general error mapping | AGENTS §§3–6, 9–12, 16; errors; security authorization/privacy; Bifrost terminal contract | Existing derive-backed catalog and general mapper reused. SQL caller faults retain admitted-table diagnostics; physical failures keep safe fixed remediation; resource classification is preserved. Mapper test now documents panic assertions. **PASS**. |
| `oracle/live.rs` pushdown, snapshot, route selection and lazy stream | AGENTS §§3–6, 10–12, 16; DataFusion, OLAP, Arrow and reliability references | Existing `LiveScribeExec` owns predicate/routes; `LiveFragmentRead` owns lazy fragment streaming. Pure snapshot/pruning remains synchronous, IO remains stream-polled. Production methods/fields document timing, partition ownership, re-execution and errors; dropping the stream retains existing cancellation behavior. Unsupported propagation preserves authoritative operators. **PASS** for standards; semantic proof belongs to implementation/domain reviews. |
| `resources.rs` shared session optimizer passes | AGENTS §§3–6, 16; DataFusion planning authority | Two passes extend existing `OracleSessionShape::apply`, with documented rule-order rationale. No second planner, classifier, runtime, public setting or resource owner. **PASS**. |
| Cargo patches/lock and `oracle/codec.rs` | AGENTS §§1–4, 11–12; native dependency-cone and peer codec authority | All eleven workspace patches, resolved DataFusion lock sources and `ORACLE_DATAFUSION_REVISION` use `f059acf6…`. Arrow and distributed pins remain unchanged. No feature, wildcard, per-crate profile or new production/client dependency. **PASS**. |
| `dispatcher.rs`, server `state.rs`, `peer_service.rs` | AGENTS §§3–6, 9–12, 16; patterns; server listener and test-support boundaries | Existing output tally exposes rows; existing Scribe owner records footer totals and latest encoded plan. Plan retention/footer-row state are behind existing `test-support`; production continues existing counters. No public wire field or parallel tenant/audit authority. New fields/methods have intent documentation. **PASS**. |
| `query_fixture.rs`, `write.rs`, `peer_cluster.rs` | AGENTS §§3–6, 11–12, 16; typed observation identity; testing-workflows | Existing writer accepts optional typed `RunId`; managed stamping remains server-owned and existing callers pass explicit None where appropriate. Layout/fragment observation extend existing cluster. Fallible helpers document errors. **PASS**. |
| `live_subquery.rs`, module wiring and latest negative journey | AGENTS §§11–12, 16; external journey placement and real runtime ownership | Existing Oracle journey binary and managed Postgres lane reused. New `LiveTables::register`/`ingest`, schema helper, test and driver have substantive rustdoc; test documents Panics and driver Errors. One key/client is reused for both queries. **PASS**. |
| Rust SDK query journeys, fixed corpus, integration module and mise selector | AGENTS §§2–3, 11–12, 16; testing-workflows | Real SDK/server paths, fixed expected rows and small mixed-state fixtures; no second DataFusion oracle or workload framework. Corpus/refusal helpers and fields have rustdoc. Owning journey selector includes `query_coverage`. **PASS**. |
| Python query journeys | AGENTS §§8, 11–12, 16; Python API and testing references | Public `wyrd` imports, top-level typed tests, existing real server fixture, structured Wyrd error assertions. No PyO3 implementation, exports or generated stubs changed. **PASS**. |
| TypeScript query journeys | AGENTS §§2–3, 11–12; TypeScript and testing references | Existing serverTest, facade, Zod/BigInt result projection and structured error assertions. No native bindings/declarations or duplicate transport/durable behavior. **PASS**. |
| MCP query journeys | AGENTS §§9, 11–12, 16; agent-harness and errors | Mounted real tool path, existing problem envelope and environment-owned ignore markers. Expanded error test now has Panics docs; scalar/IN/EXISTS test retains Errors/Panics docs. No catalog/permission changes. **PASS**. |
| Fork scalar snapshot | DataFusion `PhysicalExpr::snapshot`, native Literal and field metadata conventions; Wyrd DataFusion boundary | Existing expression implements its native snapshot hook; tests cover readiness, metadata, typed NULL and reset. No Wyrd evaluator/node. **PASS** for placement and conventions. |
| Fork scalar-to-join and `single_value` aggregate | DataFusion optimizer/registry/AggregateUDF/Accumulator/proto conventions | Extension lives at native rewrite producer; registered aggregate uses ordinary partial state and existing codec. Explicit metadata-preserving return field, typed state, native execution refusal; no Wyrd error or resource owner in the fork. **PASS** for standards. |
| Fork SQL/proto/config/docs | Native DataFusion sqllogictest, proto roundtrip, config and user-doc generation | SQL corpus expands flag-off scenarios; proto test uses existing roundtrip helper; config/user-function documentation matches registration. Evidence records owning generators regenerating docs. **PASS**. |
| Active packet/reviews/evidence | AGENTS §14; spec-driven development | Artifacts remain tracked under existing active change; no task IDs injected into runtime source. Prior trailing whitespace is removed. **PASS**. |

## Explicit rule results and prior closure

| Rule | Result |
|---|---|
| Existing owner, narrow dependency cost, no client/server duplication | **PASS**: Vala runtime, server peer serving, test-only cluster, native fork producer and SDK journeys remain distinct. `wyrd-testing` has DataFusion only in dev-dependencies. |
| Tenant SQL capability types and transaction ownership | **PASS / no new SQL authority**: cumulative delta introduces no PgPool/PgConnection/transaction fields or signatures, hand-written tenant query, commit or rollback. Existing authorized cut/context remain required before dispatch/IO. |
| Single planner/root and no fallback | **PASS**: shared session configuration changes; no new root classifier, second physical build, flag reversal or fallback. |
| Stable generated error contract | **PASS**: existing catalog/mapper is reused. No generated Wyrd schema, stub or declaration is edited. Native scalar execution failure retains existing generic execution mapping. |
| Async boundaries and runtimes | **PASS**: new pure work is synchronous; async fixture functions await registration/ingest/query IO. No new runtime or background task. |
| Rustdoc including tests | **PASS**: newly changed Wyrd items have meaningful intent and relevant Errors/Panics documentation. Prior FIND-4 is closed at `oracle/mod.rs:4473` and MCP `query.rs:202` by actual source, not a green lint claim. |
| No gate circumvention | **PASS**: new ignores designate repository-managed journeys and their selectors run ignored tests. No suppression, weakened assertion, sleep, retry or timeout change. Key-collision repair reuses one authenticated client and preserves both query assertions. |
| Language runtime/test tier placement | **PASS**: Rust-only logic stays in Rust units; server/peer workflows use existing external targets; Python and Node lifetimes stay in their language tests. |
| Cumulative whitespace gate | **PASS**: independently ran `git diff --check 526355fb8 dc353ef5e`, exit 0. |

No material repository-standard finding is proposed. Prior non-blocking
function-local imports in the scalar fixture (`live.rs:1080–1094`) and
fully-qualified fixture signature types (`query_coverage.rs` writer and
`peer_cluster.rs` layout helper) remain small style deviations without a
demonstrated behavioral/public-contract consequence; they do not justify a
new remediation round under the review threshold.

## Verification limits

Committed R2 evidence records exact Wyrd units, codec group, three remote
journeys and corpus, plus fmt/lints/codegen/dependency/tenant gates. Selectors
and test registration correspond to current source. Fork evidence records
SQL, optimizer, aggregate and proto suites, clippy/fmt and doc generators.
These are recorded results; no Cargo test, benchmark, codegen or external
environment lane was rerun in this standards discovery pass.

Earlier Python/TypeScript/MCP journey evidence applies to unchanged language
test source but does not substitute for later integrated qualification of
the final fork pin. This bounded review does not demand the complete
capability aggregate prematurely. The prior untraced memory observation and
its passing rerun remain a verification limitation, not an invented diagnosis.

## Overall result

**PASS** — applicable production/test ownership, native dependency placement,
language projection, documentation and verification-wiring standards are
satisfied; proposed material finding ledger is empty. Task acceptance and
runtime semantic completeness are reserved for the independent corresponding
reviewers and final validation.
