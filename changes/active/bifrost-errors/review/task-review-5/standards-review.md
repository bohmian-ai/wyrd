# Repository standards review — round 5

## Subject and result

**PASS. No material repository-standards findings.**

Wyrd candidate `8a54d81c346db464b624e81ebe8958c63bb7e228`, cumulative
integration baseline `945a1548c47897dc94583169a6ba04163a73a749`; original
base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`. Native fork reviewed from
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` through
`8c3f18e9806ae23e8840a710af775c058dfad66a`, with the latest
`b2d37bc18…` → `8c3f18e98…` delta inspected separately. The distributed
fork is unchanged. HEAD still matched the candidate when this report was written.

This is the independent repository-rule audit. It does not replace task
acceptance, DataFusion correctness validation, or the final finding ledger.
No current discovery report was used as evidence.

## Authority and changed-surface coverage

| Changed surface | Governing authority read and applied | Inspected evidence |
|---|---|---|
| All Wyrd Rust, manifests and packet | AGENTS.md §§1–16,18; architecture/agent-rules.md; reference router; spec-driven-development; maintainer-style; rust-core; architecture/patterns | Full cumulative executable/configuration diff, surrounding owners, latest remediation evidence; original approved spec and prior remediation context |
| Oracle logical/physical error boundary | wyrd-design Doctrine/client model/Bifrost/error catalog; wyrd-doctrine.mdx; languages/errors; domain/datafusion; Bifrost Query and public surface | oracle/mod.rs logical_plan, caller_planning_fault, map_sql_planning_error, map_query_planning_error and mapper test; existing derive-backed BifrostError projection |
| Live scan, dispatcher, shared session shape | Bifrost immutable planning, distributed execution, terminal contract; domain/datafusion and olap-serving; AGENTS async/ownership rules | live.rs resolved, pushdown, execute and LiveFragmentRead; resources.rs OracleSessionShape; NativeOutputTally; actual planning/execution ownership |
| Server fragment instrumentation and fixture ownership | AGENTS test-support/dependency/tenancy boundaries; security-posture Authorization, Peer identity, Tenant isolation and Audit; architecture/patterns | peer_service.rs execution/footer calls; state.rs test-support fields/accessors; query_fixture.rs/BifrostWriter correlation; peer_cluster registration and observations |
| Rust SDK and remote journeys | AGENTS §§11,16; TESTING.md; languages/testing-workflows; wyrd-design principle 20 | query_bifrost.rs, query_coverage.rs, integration/main.rs; peer_network/live_subquery.rs and its module wiring; reused Deployment, PeerCluster, public client and server fixtures |
| Python tests | AGENTS Python/runtime/test rules; python-api-and-stubs; testing-workflows; public client doctrine | Entire test_query_bifrost.py delta, public wyrd imports, integration marker, real server dependency, typed error assertions and run_id correlation |
| TypeScript tests | AGENTS runtime/test rules; typescript-guide; errors; testing-workflows | Entire query-bifrost.test.ts delta, serverTest file fixtures, public Bifrost/WyrdClient, error narrowing and bound async lifecycle |
| MCP tests | AGENTS MCP/audit/test rules; languages/agent-harness; errors; testing-workflows | query.rs changed negative test and live-table journey, real MCP lifecycle/tool calls and structured result/problem assertions |
| Pins, lock, codec and mise | AGENTS dependency-cost and verification rules; domain/datafusion native-universe/codec boundary; TESTING.md | Cargo.toml patch group, Cargo.lock sources, ORACLE_DATAFUSION_REVISION, existing codec test, observe journey selector includes query_coverage |
| Native fork Rust/SQL/docs | Fork AGENTS.md and existing native module conventions; Wyrd domain/datafusion integration constraints | Filter::is_scalar/is_fixed_value and tests; max_rows bounds; scalar snapshot; single_value accumulator/default registry; scalar-to-join shared build_join; proto test; subquery/information_schema SQL changes; configuration and aggregate docs |

The fork is a separate repository governed by its own contributor rules and
native conventions. Wyrd's private-item rustdoc policy is not retroactively
imposed on Apache DataFusion modules or their in-module tests.

## Applicable rule results

| Rule | Result | Source evidence and assessment |
|---|---|---|
| Server/core behavior remains Rust-owned; SDKs project one client | PASS | Production changes remain in vala-bifrost-redux and wyrd-server. Language changes are journeys through existing public clients; no second planner, transport or durable client implementation was added. |
| Invariant/stateful behavior has an existing concrete owner; pure helpers remain synchronous | PASS | Predicate snapshot and routes remain on LiveScribeExec, stream lifetime on LiveFragmentRead, options on OracleSessionShape, footer count on NativeOutputTally. Error classifiers and native expression proof are synchronous pure helpers. Async fixture methods await real setup/ingest/query IO. |
| Actual SQL capability types, not merely a green grep gate | PASS | Touched production symbols introduce no SQL pool/connection parameter or field. Surrounding AppState registry_tenant_conn at state.rs:2402 returns TenantConn; raw PgPoolOptions occurrences at :2605/:2783 are within the existing test module. The new test registration method delegates the tenant-qualified catalog; it does not accept a raw pool or commit a caller transaction. |
| Derived stable error catalog, redacted internal failures | PASS | mod.rs maps into existing QueryInvalidSql, QueryResourcesExhausted and QueryExecutionFailed variants. It adds no hand-written public metadata/problem serializer. Logical planner detail is separated from physical/dependency diagnostics; private wrappers stay out of the returned detail. |
| No new trust, audit, listener or storage authority | PASS | Peer service still starts the authenticated existing fragment executor. Added observation is test-support-gated and does not alter request context, authorization, snapshot identity, audit or durable writes. |
| Test-only machinery stays out of shipped dependencies | PASS | Scribe plan bytes and footer-row observations are cfg(test-support); normal execution retains the existing counters and ignores observation arguments. No new feature or dependency was added by the latest fix; client-tier production dependency edges are unchanged. |
| One native DataFusion universe and explicit codec revision | PASS | All eleven root DataFusion patches, every DataFusion git lock source and codec.rs:32 agree on 8c3f18e9806ae23e8840a710af775c058dfad66a. Arrow and distributed revisions are unchanged. |
| Wyrd rustdoc, fallible operations, panics and lifecycle | PASS | New/materially changed Wyrd owners, fields, accessors, helpers and tests carry intent/invariant docs. Fallible fixture/proof helpers have Errors sections; assertion/expect tests have Panics sections. The two prior missing Panics sections exist on the mapper and MCP negative test. Live stream docs explain cancellation, footer loss and failure precedence; execute docs describe snapshot failure. |
| Tests use their owning runtime and appropriate tiers | PASS | Pure snapshot/error mapping stay in Rust unit tests; real server/SDK/MCP tests stay in gated journey targets. Python and Node lifetimes are exercised by their language tests. No embedded foreign runtime or in-process DataFusion replacement for a Wyrd user journey was added. |
| Generated source ownership | PASS | No generated stub, declaration, schema or protocol payload was hand-edited. No language export/signature change requires an additional binding layer. Fork aggregate docs and native registry/SQL information-schema expectations move with single_value. |
| Test registration and canonical verification | PASS | Rust query_coverage is an ordinary integration module and is included by the owning observe selector. Remote journeys are ordinary modules of the Oracle target. Latest evidence records nonzero focused counts 2/4/3/1, fork exact unit and SQL selection, fmt/lints/codegen/deps/tenant checks; no assertion/timeout/retry was weakened in the latest fix. |
| Review immutability and scope | PASS | Read-only source inspection, report-only write. No build, benchmark, test execution, dependency mutation, commit or push was performed by this reviewer. |

## Verification evidence and limits

The latest committed R4 task packet supplies exact commands, selected counts,
red/green descriptions and final checks. Source inspection confirms the named
unit test, six-query Analytical refusal journey, corpus and codec tests exist
and that the latest change does not weaken their assertions. The reported
native 520-file run is additional evidence, not a new requirement imposed here.
Earlier packet evidence covers the unchanged Python, TypeScript and MCP
journeys; those were not rerun for this native-proof/pin-only remediation.

No commands were rerun by this reviewer. Runtime correctness and the claim
that conservative expression handling preserves all valid scalar queries are
for the independent behavior/invariant/domain reviewers and validation. The
standard audit makes no benchmark or performance-measurement claim.

Material proposed finding ledger: **empty**.
