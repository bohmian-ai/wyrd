# Independent repository standards review — round 6

## Subject and result

**PASS. No material repository-standards findings.**

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`95b350d078226e811ac298230c233bf935975d42`, with original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79` retained as history. Reviewed the
native DataFusion cumulative range `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
→ `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`, inspecting the latest
`8c3f18e9806ae23e8840a710af775c058dfad66a` → candidate delta separately.
The distributed fork remains pinned at `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.
Both checked-out HEADs matched these candidates at report preparation.

This is the fresh repository-rule audit, not the task-acceptance or native
semantic review. No current discovery report or implementation completion
summary supplied a conclusion. CodeGraph was consulted before Wyrd code
navigation; its broad result was supplemented by targeted source/diff reads.

## Authority and coverage

| Changed surface | Governing authority applied | Source coverage |
|---|---|---|
| Cumulative Wyrd code, packet and manifests | AGENTS.md, architecture/agent-rules.md, reference router; spec-driven-development, implementation-execution, maintainer-style and rust-core | Entire cumulative executable/configuration delta; approved revision-1 spec; R1–R5 remediation context and latest recorded command/evidence table |
| Oracle logical and physical errors | wyrd-design doctrine, client model, Bifrost and error catalog; wyrd-doctrine; languages/errors; agent-harness; Bifrost Query/public surface | oracle/mod.rs logical_plan, caller_planning_fault, map_sql_planning_error, map_query_planning_error and error-mapper test; existing derive-backed BifrostError variants |
| Live predicate, fragment streaming and session options | Bifrost immutable planning, distributed execution, terminal/resource invariants; domain/datafusion and olap-serving; Rust ownership/async rules | LiveScribeExec::resolved, pushdown and execute; LiveFragmentRead::into_stream; OracleSessionShape::config; NativeOutputTally::rows and footer completion |
| Server instrumentation and test helpers | AGENTS dependency/test-support/tenancy rules; architecture/patterns; security-posture trust/tenant/audit boundaries | Scribe fields/construction/fragment observations, ScribeFragmentExecutor call sites, BifrostWriter enqueue/write, WyrdTestServer query fixture, PeerCluster registration and Scribe observations |
| Rust SDK and remote journeys | AGENTS §§11 and 16; TESTING.md; testing-workflows; public client doctrine | query_bifrost.rs, complete query_coverage.rs, integration/main.rs; complete peer_network/live_subquery.rs plus parent module and PeerCluster support |
| Python journey changes | AGENTS Python/runtime/testing rules; python-api-and-stubs; errors; testing-workflows | Entire test_query_bifrost.py delta, public imports, module fixture/marker, run_id writes, exact error-code/detail and unauthorized-table assertions |
| TypeScript journey changes | AGENTS runtime/testing rules; typescript-guide; errors; testing-workflows | Entire query-bifrost.test.ts delta, public clients, file-scoped server fixtures, typed result schemas, error assertions and async writer lifecycle |
| MCP journeys | AGENTS MCP/testing rules; agent-harness; errors; testing-workflows | query.rs changed planning-error test and new live-subquery journey, real tool transport/lifecycle, structured problems/results |
| Dependency revision and lane registration | AGENTS dependency cost/generation/test policy; native-universe/codec requirements in domain/datafusion; TESTING.md | Cargo.toml complete patch group, Cargo.lock source changes, codec revision and existing compatibility fence, mise observe selector and query_coverage module registration |
| Native fork Rust/SQL/docs | Fork AGENTS.md/contributor entry points and native surrounding conventions; Wyrd native integration constraints | Filter::is_scalar/is_fixed_value and test family; max_rows changes; single_value type/accumulator/registry; scalar-to-join shared build_join; ScalarSubqueryExpr snapshot; native proto/SQL/config/aggregate documentation changes |

The separate Apache DataFusion repository has its own contributor conventions.
Wyrd's private-item rustdoc rule is applied to Wyrd items; it is not imposed
retroactively on native DataFusion trait implementations and tests. No frontend,
new wire contract, generated binding, migration, storage format, or deployment
surface was changed in the reviewed ranges.

## Applicable rule results

| Rule | Result | Source evidence |
|---|---|---|
| Server/core ownership and single shared client | PASS | Production Wyrd behavior remains in vala-bifrost-redux and wyrd-server. SDK files add public-client journeys, not planners, transports or durable behavior. Native cardinality behavior stays in DataFusion. |
| Meaningful existing owner; pure computation stays synchronous | PASS | LiveScribeExec owns predicate/routes, LiveFragmentRead owns stream lifetime, OracleSessionShape owns shared configuration and NativeOutputTally owns counts. Mapper and fixed-expression functions are synchronous calculations. New async helpers actually await registration/ingest/query or fixture IO. |
| Actual SQL capability types | PASS | New/materially changed runtime methods introduce no raw pool, connection or caller transaction fields/parameters. AppState::registry_tenant_conn (state.rs:2399–2407) returns TenantConn through the retained Postgres handle. PgPoolOptions occurrences at state.rs:2605/2783 are in existing tests. New PeerCluster::register_table_with_layout delegates to the tenant-qualified catalog using self.tenant(); it does not construct a pool or own caller commit/rollback. |
| Stable derived public errors and safe diagnostic boundary | PASS | New mappings reuse QueryInvalidSql, QueryResourcesExhausted and QueryExecutionFailed. No hand-written error catalog/problem serializer was added. Logical caller detail is separated from physical operator diagnostics; general/internal paths retain opaque errors, wrappers are not returned as detail. Authorization/privacy correctness is independently reviewed by the domain role. |
| Existing authority, audit and listener boundaries | PASS | Peer instrumentation runs after existing authenticated fragment startup; it adds no authorization decision, audit writer, listener or durable storage authority. The live read still receives existing execution bindings/grants and uses the existing dispatcher/terminal decoder. |
| Test-only data stays behind established feature | PASS | Additional retained plan bytes/footer-row observations are cfg(test-support). Production paths keep their existing counters and ignore the new observation arguments. No new Cargo feature or production-client dependency is introduced. |
| One native analytical dependency universe | PASS | Eleven root DataFusion patches, native git lock sources and codec.rs:32 agree on b8ec4b1d291cd8e371bb88fc76959d91e8503a1d. Arrow and distributed pins remain unchanged; no bridge, protocol node or codec replacement was added. |
| Wyrd rustdoc including fallibility and panics | PASS | New/materially changed Wyrd fields/methods/helpers/tests have substantive role/invariant documentation. Fallible proof/setup helpers document Errors; assertion-based tests document Panics. Mapper/MCP Panics corrections remain present. Snapshot and live stream documentation describes lazy execution, re-execution, cancellation and failure behavior. Latest Lookup/result fields and extended join proof have corresponding docs. |
| Test runtime and tier ownership | PASS | Pure mapper/snapshot tests remain Rust unit tests. Cross-server/SDK/MCP tests are in real gated journey targets. Python/Node lifetimes are exercised through owning language tests; no foreign runtime was embedded into Rust. The extended remote join uses the public client and reuses its existing seeded topology. |
| Existing fixture ownership and failure diagnosis | PASS | query_coverage uses Deployment and public Bifrost; remote tests extend PeerCluster. Latest join journey provisions one key/client for two queries. R5 evidence diagnoses its first duplicate-identity failure as repeated helper provisioning, consistent with the helper's provision_public_api_key call and the new direct client reuse. No query assertion, timeout, retry or error acceptance was weakened. |
| Native expression traversal and dependency cost | PASS | Latest change extends the existing fixed-value calculation and uses Expr::apply_children; it adds no state store, evaluator, proof framework, trait, dependency or configuration. The native test remains in the owning module and SQL checks remain in subquery.slt. Semantic completeness belongs to the independent native/implementation audits. |
| Generated artifact source ownership and language surfaces | PASS | No .pyi, generated TypeScript declaration, schema or protocol artifact was hand-edited. Language changes are typed tests using public exports. Native aggregate docs and information_schema expectation accompany the native default registration. |
| Canonical test registration and exact evidence | PASS | Rust query_coverage is registered once under integration/main.rs and the observe selector; remote tests are ordinary modules in the Oracle target. Latest evidence records the exact fork unit selector, subquery.slt, nonzero native suites, Wyrd 3/1/2/4 focused counts and fmt/lints/codegen/deps/tenant checks. Prior evidence covers unchanged language/MCP surfaces. Final integrated gates remain the later change-review responsibility. |
| Immutable review and bounded scope | PASS | This reviewer changed only this report. No source edits, Cargo builds, tests, benchmarks, synthetic load, commits or pushes were performed. |

## Verification assessment and limits

The R5 task evidence records the required failing CASE proof, exact unit and
one-file SQL commands, the three formerly refused valid SQL shapes, negative
row-dependent/volatile cases, and the public grouped CASE lookup with an
unmatched row. Source confirms these tests/assertions and the unchanged native
refusal controls exist. Its current-pin result counts are expr 265, optimizer
886, single_value 4, proto lib 17, three remote journeys, one corpus, two Wyrd
unit tests and four codec tests. Reported checks are fmt/clippy in the native
changed crate and Wyrd fmt/lints/codegen/dependency/tenant checks.

These are inspected implementation records, not fresh executions by this
reviewer. Earlier round records cover unchanged cumulative surfaces. No full
native workspace run, complete Bifrost closeout gate or benchmark was rerun
here, and no measured performance claim follows from this standards result.

Material proposed findings: **none**. Overall: **PASS**.
