# Round-6 tenant and security domain review

**PASS.** No material proposed findings in the reviewed tenant/security boundary.

## Immutable subject and scope

Wyrd candidate `95b350d078226e811ac298230c233bf935975d42`, cumulative integration baseline `945a1548c47897dc94583169a6ba04163a73a749`; original base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`. Native DataFusion cumulative `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` to `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`, with latest remediation located using previous native `8c3f18e9806ae23e8840a710af775c058dfad66a` and Wyrd `8a54d81c346db464b624e81ebe8958c63bb7e228`. HEAD was checked at the end and matches the candidate. No source edits or test/build execution.

Reviewed the cumulative error disclosure and SQL authorization path, live predicate materialization and remote fragment authority, published/live tenant identity, and the new correlated grouped CASE journey. This domain report does not substitute for the native expression/cardinality acceptance audits.

## Governing authority and source coverage

| Authority / obligation | Source examined | Assessment |
|---|---|---|
| Approved `spec.md` REQ-002: caller planning details, no unauthorized schema disclosure; INV-001; AC-002 | `oracle/mod.rs` `caller_planning_fault`, `map_sql_planning_error`, `map_query_planning_error`, `map_datafusion_error`; logical-plan entry; Rust unauthorized-table journey | PASS: logical caller causes are exposed through the existing invalid-SQL catalog variant; physical diagnostics and internal/execution failures retain scrubbed mappings. |
| `AGENTS.md`, `architecture/agent-rules.md`: verified tenant authority and SQL capability owners | `oracle/planner.rs` request validation and `read_and_materialize`; `oracle/mod.rs` `authorize_resolved_tables`, `register_cut_providers`, `register_session_table` | PASS: authenticated `data_tenant_id` drives cut lookup and physical binding; every resolved object is authorized before materialization/provider registration. No new raw SQL pool, transaction owner or manual tenant-selection mechanism is introduced. |
| `architecture/wyrd-security-posture.md`: scoped SQL object authorization, receiver-owned peer context, fail closed on mismatch | `oracle/mod.rs` `authorize_payload_columns`/`scoped_permission_digest`; `oracle/live.rs` `fragment`, `LiveFragmentRead::stream`, failure classification; server `oracle/peer_service.rs` Scribe fragment executor | PASS: the existing object/payload checks and permission digest remain authoritative; worker verifies ticket, tenant/bindings, fences, protocol and exact assignment authority before executable plan/provider IO. |
| `architecture/bifrost-design.md` tenant-qualified physical tables and bounded query identity; `references/domain/olap-serving.md` | `LiveTableRoutes::selected_by`, `LiveScribeExec::resolved`, `fragment`; pinned provider registration and published-source context | PASS: pruning only removes routes from the existing bound table. Snapshotting cannot introduce a new table, tenant or route. |
| `references/languages/errors.md`: existing derive-backed error contract and no raw dependency internals | Oracle mapping and existing shared client/MCP journeys; public catalog conversion is unchanged | PASS: no parallel public error mapper or new native diagnostic exposure. |
| Owner-directed root correction and valid correlated behavior; prior R5 verdict/task | Native `Filter::is_scalar` and `is_fixed_value`; scalar rewrite shared `build_join`, physical scalar snapshot and native `single_value`; latest peer journey | PASS within domain: native proof changes affect cardinality classification, with no independent tenant lookup, credential source, catalog or authorization bypass. |

CodeGraph was consulted before locating code. The repository skill, agent rules and reference router were read; applicable error/OLAP/security guidance was applied alongside the approved spec and prior R5 packet. Current discovery reports were not read.

## End-to-end boundary evidence

### Authorized schema details

`parse_select_tables` (`oracle/mod.rs:3233`) resolves references from the complete statement through DataFusion's native resolver, including nested queries and CTE relations. `OraclePlanner::read_and_materialize` (`oracle/planner.rs:104`) uses the authenticated context tenant, resolves typed physical identities, calls `authorize_resolved_tables` before object materialization, and only then returns cuts for provider registration. `register_cut_providers` (`oracle/mod.rs:2623`) registers those cuts, preserving authenticated context on each provider. `register_session_table` (`:3142`) installs canonical names and the quoted-dotted alias for the same provider; it does not register a global catalog.

The SQL mapping (`:3745–3789`) unwraps typed caller causes and strips backtraces, rather than revealing arbitrary external errors or enclosing context. Resources retain precedence. Physical mapping (`:3798`) uses a fixed repair message because physical errors may name internal operators; general execution mapping (`:3814`) preserves catalog identities and otherwise returns opaque execution failure. A joined forbidden table cannot supply suggestion columns: `authorize_resolved_tables` (`:3426`) refuses before the schema is registered. The Rust SDK `unauthorized_table_refusal_reveals_no_columns` (`query_bifrost.rs:402`) proves an allowed-table read and then a forbidden joined table with a misspelled column, asserting the 403 and absence of both the real and misspelled column names.

`authorize_payload_columns` (`oracle/mod.rs:3463`) continues using `apply_with_subqueries` on the optimized plan. New CASE support adds no alternative path around that gate. The new grouped lookup is ordinary SQL passed to one authenticated public Bifrost client.

### Native proof and live execution

The native fixed-value classifier admits literal/parameter/outer-reference leaves and enumerated pure forms only when every child is fixed; volatile scalar functions and unlisted forms do not grant proof. Outer references are fixed per inner invocation, not a different tenant identity. Neither this classifier nor `max_rows` obtains providers or grants permissions. Its sibling consumers remain the existing scalar cardinality rewrite and correlated validation paths.

The cumulative native scalar snapshot materializes only the value already supplied by that expression's results container. Wyrd `LiveScribeExec::resolved` (`live.rs:298`) copies the original table binding, deals each route to its original output partition, snapshots the predicate, and removes impossible routes from that share. `LiveTableRoutes::selected_by` (`:111`) always clones the original binding and filters the existing route vector. No expression-derived tenant/table identity enters this operation. The leader retains authoritative operators because pushdown reports unsupported.

`LiveFragmentRead::stream` (`:668`) first requires execution bindings and a live grant; it dispatches the resolved leaf's fragment using the pre-bound live context. `fragment` (`:331`) encodes the narrowed predicate over an identity-bound placeholder; the binding and provider cut remain assignment inputs. On Scribe, the fragment executor (`peer_service.rs:270–348`) verifies the signed ticket, compares assignment tenants to the verified tenant, checks named binding/fences/protocol/digests, recomputes assignment-authority digest, and runs authenticated preflight before execution. Snapshot literals change the signed plan fingerprint, not the authority source.

Tenant/security/protocol/resource failures remain fatal: `is_availability_loss` (`live.rs:790`) only admits designated availability losses; `live_error` preserves the typed tenant invariant (`:803`). Foreign tenant evidence cannot become a degraded successful result.

### Test-only evidence retention

Cumulative `state.rs` latest-plan bytes and footer-row counters are behind `test-support`; execution recording occurs after authenticated follower execution begins. They do not add a public discovery endpoint or production cross-query plan cache. The existing production counters retain their previous role.

The latest journey change provisions one public API key and reuses its client for the original join plus grouped CASE lookup. This fixes test setup duplication without broadening grants, weakening result assertions, modifying refusal assertions, or changing request authorization. It asserts both a found `(107,108)` lookup and an unmatched `(199,NULL)` lookup over the same tenant-bound live events table.

## Verification assessment and limits

The R5 task evidence records exact native expression/SQL controls and the current Wyrd three peer journeys, corpus, mapper/snapshot units, codec tests and format/lint/codegen/dependency/tenant checks. Relevant assertions were inspected in source; these executions were not repeated during this static review. Earlier first-class SDK/MCP query tests and the explicit Rust unauthorized-table journey remain in the cumulative diff. No new negative multi-tenant deployment test is asserted as having run here.

No benchmark, broad test lane, synthetic load, external live-service probe, build, or production deployment was run. Available passing checks support the reviewed boundary but do not replace source tracing or claim to prove all unrelated security behavior. No reachable tenant/security regression was found, and there is no domain-specific remediation recommendation.
